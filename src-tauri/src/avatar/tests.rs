//! Pruebas unitarias completas de la integración con el asset de avatar de VRChat.

use super::config::AvatarIntegrationConfig;
use super::protocol::*;
use super::rig_adapter::{AvatarPathAdapter, RigRepresentationStatus};
use super::service::{AvatarExecutionRequest, IrisIntegrationService};
use super::validator::{ParameterValidator, ValidatedAvatarCommand};
use crate::camera::CameraSnapshot;
use crate::osc::protocol::Pose;
use crate::studio::evaluator::evaluate;
use rosc::{OscMessage, OscType};

fn mock_camera_snap(pos: [f32; 3], rot: [f32; 3]) -> CameraSnapshot {
    CameraSnapshot {
        open: true,
        mode: 1,
        rx_available: true,
        pose: Some(Pose { pos, rot }),
        pose_fresh: true,
        pose_seq: 1,
        pose_epoch_ms: Some(1700000000),
        zoom: Some(45.0),
        exposure: Some(0.0),
        focal_distance: Some(2.0),
        aperture: Some(8.0),
        look_at_me: Some(false),
        smooth_movement: Some(false),
        streaming: Some(false),
    }
}

#[test]
fn test_1_parameter_reception_and_validation() {
    let mut v = ParameterValidator::new();

    // Valid action
    let m_act = OscMessage {
        addr: ADDR_ACTION.into(),
        args: vec![OscType::Int(1)],
    };
    let res = v.validate_message(&m_act).unwrap();
    assert_eq!(res, Some(ValidatedAvatarCommand::Action(AvatarAction::CaptureA)));

    // Valid duration
    let m_dur = OscMessage {
        addr: ADDR_DURATION.into(),
        args: vec![OscType::Float(0.5)],
    };
    let res = v.validate_message(&m_dur).unwrap();
    assert_eq!(res, Some(ValidatedAvatarCommand::Duration(0.5)));

    // Reject NaN duration
    let m_nan = OscMessage {
        addr: ADDR_DURATION.into(),
        args: vec![OscType::Float(f32::NAN)],
    };
    assert!(v.validate_message(&m_nan).is_err());

    // Valid anchor mode
    let m_anc = OscMessage {
        addr: ADDR_ANCHOR_MODE.into(),
        args: vec![OscType::Int(1)],
    };
    let res = v.validate_message(&m_anc).unwrap();
    assert_eq!(res, Some(ValidatedAvatarCommand::AnchorMode(AvatarAnchorMode::Player)));

    // Reject anchor mode out of range
    let m_bad_anc = OscMessage {
        addr: ADDR_ANCHOR_MODE.into(),
        args: vec![OscType::Int(99)],
    };
    assert!(v.validate_message(&m_bad_anc).is_err());

    // Valid loop bool
    let m_loop = OscMessage {
        addr: ADDR_LOOP.into(),
        args: vec![OscType::Bool(true)],
    };
    let res = v.validate_message(&m_loop).unwrap();
    assert_eq!(res, Some(ValidatedAvatarCommand::Loop(true)));
}

#[test]
fn test_2_correct_duration_conversion() {
    let min = 1.0;
    let max = 30.0;

    assert_eq!(normalized_to_duration(0.0, min, max), 1.0);
    assert_eq!(normalized_to_duration(1.0, min, max), 30.0);
    assert_eq!(normalized_to_duration(0.5, min, max), 15.5);

    // Roundtrip
    let norm = duration_to_normalized(15.5, min, max);
    assert!((norm - 0.5).abs() < 1e-4);
}

#[test]
fn test_3_single_execution_and_rearm_logic() {
    let mut v = ParameterValidator::new();

    let m_exec = OscMessage {
        addr: ADDR_ACTION.into(),
        args: vec![OscType::Int(4)], // Execute
    };

    // First trigger fires
    let r1 = v.validate_message(&m_exec).unwrap();
    assert_eq!(r1, Some(ValidatedAvatarCommand::Action(AvatarAction::Execute)));

    // Duplicate message with same action WITHOUT rearming is ignored
    let r2 = v.validate_message(&m_exec).unwrap();
    assert_eq!(r2, None);

    // Send Inactive (0) to rearm
    let m_zero = OscMessage {
        addr: ADDR_ACTION.into(),
        args: vec![OscType::Int(0)],
    };
    let r3 = v.validate_message(&m_zero).unwrap();
    assert_eq!(r3, None); // Inactive does not emit a command, but rearms

    // Now it can trigger again
    let r4 = v.validate_message(&m_exec).unwrap();
    assert_eq!(r4, Some(ValidatedAvatarCommand::Action(AvatarAction::Execute)));
}

#[test]
fn test_4_duplicate_message_handling() {
    let mut v = ParameterValidator::new();

    let m_dur1 = OscMessage {
        addr: ADDR_DURATION.into(),
        args: vec![OscType::Float(0.5)],
    };
    let m_dur2 = OscMessage {
        addr: ADDR_DURATION.into(),
        args: vec![OscType::Float(0.5002)], // difference < 1e-3
    };

    assert!(v.validate_message(&m_dur1).unwrap().is_some());
    assert_eq!(v.validate_message(&m_dur2).unwrap(), None); // deduplicated
}

#[test]
fn test_5_avatar_change_detection() {
    let mut v = ParameterValidator::new();
    let m_change = OscMessage {
        addr: ADDR_AVATAR_CHANGE.into(),
        args: vec![OscType::String("avtr_unit_test_999".into())],
    };
    let res = v.validate_message(&m_change).unwrap();
    assert_eq!(
        res,
        Some(ValidatedAvatarCommand::AvatarChange("avtr_unit_test_999".into()))
    );

    // Duplicate avatar id is ignored
    assert_eq!(v.validate_message(&m_change).unwrap(), None);
}

#[test]
fn test_6_reject_invalid_actions() {
    let mut v = ParameterValidator::new();
    let m_invalid = OscMessage {
        addr: ADDR_ACTION.into(),
        args: vec![OscType::Int(15)], // 15 is outside 0..6
    };
    assert!(v.validate_message(&m_invalid).is_err());
}

#[test]
fn test_7_block_execution_with_incomplete_trajectory() {
    let cfg = AvatarIntegrationConfig::default();
    let mut s = IrisIntegrationService::new(cfg);

    // No points: Execute must fail
    assert!(s.handle_action(AvatarAction::Execute).is_err());
    assert_eq!(s.state, AvatarIntegrationState::Error);

    // Capture A only
    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap();
    assert_eq!(s.state, AvatarIntegrationState::Configuring);

    // Execute with only A: must fail and report error
    assert!(s.handle_action(AvatarAction::Execute).is_err());
    assert_eq!(s.state, AvatarIntegrationState::Error);
}

#[test]
fn test_8_separation_between_preview_and_real_execution() {
    let cfg = AvatarIntegrationConfig::default();
    let mut s = IrisIntegrationService::new(cfg);

    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    let snap_b = mock_camera_snap([5.0, 1.6, 0.0], [0.0, 90.0, 0.0]);

    s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap();
    s.handle_action_with_camera(AvatarAction::CaptureB, Some(&snap_b), None).unwrap();
    assert_eq!(s.state, AvatarIntegrationState::Ready);

    // Preview
    let req_prev = s.handle_action_with_camera(AvatarAction::Preview, None, None).unwrap();
    assert_eq!(req_prev, Some(AvatarExecutionRequest::StartPlayback { live_osc: false }));
    assert_eq!(s.state, AvatarIntegrationState::Previewing);

    // Reset to ready
    s.handle_action(AvatarAction::Stop).unwrap();

    // Execute
    let req_exec = s.handle_action_with_camera(AvatarAction::Execute, None, None).unwrap();
    assert_eq!(req_exec, Some(AvatarExecutionRequest::StartPlayback { live_osc: true }));
    assert_eq!(s.state, AvatarIntegrationState::Playing);
}

#[test]
fn test_9_consistent_trajectory_evaluation() {
    let cfg = AvatarIntegrationConfig::default();
    let mut s = IrisIntegrationService::new(cfg);

    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    let snap_b = mock_camera_snap([10.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap();
    s.handle_action_with_camera(AvatarAction::CaptureB, Some(&snap_b), None).unwrap();

    let adapter = AvatarPathAdapter::new(s.config.clone());
    let rig_snap = adapter.compute_rig_samples(&s.project, None, 5.0);

    // La muestra intermedia del rig debe coincidir exactamente con la evaluación matemática
    let mid_rig = &rig_snap.samples[rig_snap.samples.len() / 2];
    let mid_eval = evaluate(&s.project, None, s.project.duration * mid_rig.arc_fraction);

    assert!((mid_rig.position[0] - mid_eval.base_pose.pos[0]).abs() < 1e-3);
    assert!((mid_rig.position[1] - mid_eval.base_pose.pos[1]).abs() < 1e-3);
}

#[test]
fn test_10_spatial_sampling_and_tangent_orientation() {
    let mut cfg = AvatarIntegrationConfig::default();
    cfg.bone_count = 8;
    let mut s = IrisIntegrationService::new(cfg);

    // Straight line along +Z axis
    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    let snap_b = mock_camera_snap([0.0, 1.6, 10.0], [0.0, 0.0, 0.0]);
    s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap();
    s.handle_action_with_camera(AvatarAction::CaptureB, Some(&snap_b), None).unwrap();

    let rig = s.rig_adapter.compute_rig_samples(&s.project, None, 0.0);
    assert_eq!(rig.samples.len(), 8);

    // Tangents should point in +Z direction
    for sample in &rig.samples {
        assert!(sample.tangent[2] > 0.9, "Tangente tz esperada > 0.9 pero fue {}", sample.tangent[2]);
        assert!(sample.tangent[0].abs() < 0.05);
        assert!(sample.tangent[1].abs() < 0.05);
    }
}

#[test]
fn test_11_separation_between_displacement_tangent_and_camera_rotation() {
    let cfg = AvatarIntegrationConfig::default();
    let mut s = IrisIntegrationService::new(cfg);

    // Camera moves forward along +Z (from z=0 to z=10), but rot looks 90° sideways to the right (yaw = 90)
    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 90.0, 0.0]);
    let snap_b = mock_camera_snap([0.0, 1.6, 10.0], [0.0, 90.0, 0.0]);
    s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap();
    s.handle_action_with_camera(AvatarAction::CaptureB, Some(&snap_b), None).unwrap();

    let rig = s.rig_adapter.compute_rig_samples(&s.project, None, 0.0);

    // Displacement tangent points along +Z (yaw ≈ 0 for arrow)
    let arrow_yaw = rig.samples[1].arrow_rot[1];
    assert!(arrow_yaw.abs() < 5.0, "Flecha debe apuntar hacia el avance Z (yaw ≈ 0), fue {}", arrow_yaw);

    // But camera rot is maintained at 90°
    let cam_yaw = rig.samples[1].camera_rot[1];
    assert!((cam_yaw - 90.0).abs() < 5.0, "Cámara debe mantener orientación lateral (yaw ≈ 90), fue {}", cam_yaw);
}

#[test]
fn test_12_rig_status_reports_pending_vrc_limitations() {
    let cfg = AvatarIntegrationConfig::default();
    let s = IrisIntegrationService::new(cfg);
    let snap = s.snapshot(0.0, None);
    assert!(matches!(snap.rig_snapshot.status, RigRepresentationStatus::PendingVrcLimitations(_)));
}

#[test]
fn test_13_service_actions_and_state_sync_do_not_deadlock() {
    let cfg = AvatarIntegrationConfig::default();
    let mut s = IrisIntegrationService::new(cfg);
    let snap_a = mock_camera_snap([0.0, 1.6, 0.0], [0.0, 0.0, 0.0]);
    let snap_b = mock_camera_snap([1.0, 1.6, 2.0], [0.0, 45.0, 0.0]);

    // Sequential actions that previously could deadlock if locks were not dropped
    assert!(s.handle_action_with_camera(AvatarAction::CaptureA, Some(&snap_a), None).unwrap().is_none());
    assert_eq!(s.state, AvatarIntegrationState::Configuring);

    assert!(s.handle_duration(0.75).is_ok());

    assert!(s.handle_action_with_camera(AvatarAction::CaptureB, Some(&snap_b), None).unwrap().is_none());
    assert_eq!(s.state, AvatarIntegrationState::Ready);

    let mut adapter = super::state_adapter::AssetStateAdapter::new();
    let packets = adapter.sync_state_packets(s.state, 0.5, true);
    assert!(!packets.is_empty());

    let req = s.handle_action_with_camera(AvatarAction::Preview, None, None).unwrap();
    assert_eq!(req, Some(AvatarExecutionRequest::StartPlayback { live_osc: false }));
    assert_eq!(s.state, AvatarIntegrationState::Previewing);

    let stop_req = s.handle_action_with_camera(AvatarAction::Stop, None, None).unwrap();
    assert_eq!(stop_req, Some(AvatarExecutionRequest::StopPlayback));
    assert_eq!(s.state, AvatarIntegrationState::Stopped);
}
