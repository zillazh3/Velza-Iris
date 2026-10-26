// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Desconocido"
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "desconocida".into());
        let msg = format!(
            "[CRASH/PANIC] Pánico en {}: {}\nBacktrace:\n{:?}\n",
            location, payload, backtrace
        );
        eprintln!("{}", msg);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("VelzaIris-data").join("crash.log"))).filter(|p| p.parent().is_some_and(|d| d.is_dir())).unwrap_or_else(|| "crash.log".into())) {
            use std::io::Write;
            let _ = writeln!(f, "{}", msg);
        }
    }));

    velza_iris_lib::run()
}
