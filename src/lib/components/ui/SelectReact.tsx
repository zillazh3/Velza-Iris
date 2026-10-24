// @ts-nocheck
import React, { useState, useRef, useEffect, useCallback, useId } from "react";
import { ChevronDown, Check } from "lucide-react";

export interface SelectOption<T = string> {
  value: T;
  label: string;
  icon?: React.ComponentType<{ size?: number; className?: string }>;
  description?: string;
  shortcut?: string;
  disabled?: boolean;
}

export interface CustomSelectProps<T = string> {
  items: SelectOption<T>[];
  value?: T;
  onChange?: (value: T) => void;
  placeholder?: string;
  size?: "sm" | "md" | "lg";
  disabled?: boolean;
  name?: string;
  id?: string;
  ariaLabel?: string;
  className?: string;
  glowOnActive?: boolean;
}

export function CustomSelect<T extends string = string>({
  items,
  value,
  onChange,
  placeholder = "Seleccionar opción...",
  size = "md",
  disabled = false,
  name,
  id,
  ariaLabel,
  className = "",
  glowOnActive = true,
}: CustomSelectProps<T>) {
  const [isOpen, setIsOpen] = useState(false);
  const [placement, setPlacement] = useState<"bottom" | "top">("bottom");
  const [highlightedIndex, setHighlightedIndex] = useState<number>(-1);

  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listboxRef = useRef<HTMLUListElement>(null);

  // Typeahead state
  const searchBufferRef = useRef<string>("");
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const reactId = useId();
  const instanceId = id || `select-${reactId}`;
  const listboxId = `${instanceId}-listbox`;

  const selectedItem = items.find((item) => item.value === value);

  // Optical Size styles
  const sizeClasses = {
    sm: {
      trigger: "h-8 px-2.5 text-xs rounded-[9px] gap-2",
      iconSize: 14,
      option: "px-2.5 py-1.5 text-xs",
      shortcut: "text-[10px]",
    },
    md: {
      trigger: "h-9.5 px-3.5 text-sm rounded-[11px] gap-2.5",
      iconSize: 16,
      option: "px-3 py-2 text-sm",
      shortcut: "text-xs",
    },
    lg: {
      trigger: "h-11 px-4 text-base rounded-[13px] gap-3",
      iconSize: 18,
      option: "px-3.5 py-2.5 text-base",
      shortcut: "text-xs",
    },
  }[size];

  // Collision handling (Flip upwards if bottom space is constrained)
  const calculatePlacement = useCallback(() => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    const minSpace = 240;
    if (spaceBelow < minSpace && rect.top > minSpace) {
      setPlacement("top");
    } else {
      setPlacement("bottom");
    }
  }, []);

  const openDropdown = useCallback(() => {
    if (disabled) return;
    calculatePlacement();
    setIsOpen(true);

    const selectedIdx = items.findIndex((it) => it.value === value);
    if (selectedIdx >= 0 && !items[selectedIdx].disabled) {
      setHighlightedIndex(selectedIdx);
    } else {
      setHighlightedIndex(items.findIndex((it) => !it.disabled));
    }
  }, [disabled, items, value, calculatePlacement]);

  const closeDropdown = useCallback(() => {
    setIsOpen(false);
    setHighlightedIndex(-1);
  }, []);

  const toggleDropdown = useCallback(() => {
    if (isOpen) {
      closeDropdown();
    } else {
      openDropdown();
    }
  }, [isOpen, openDropdown, closeDropdown]);

  const selectOption = useCallback(
    (option: SelectOption<T>) => {
      if (option.disabled) return;
      onChange?.(option.value);
      closeDropdown();
      triggerRef.current?.focus();
    },
    [onChange, closeDropdown]
  );

  // Scroll highlighted item into view smoothly
  useEffect(() => {
    if (!listboxRef.current || highlightedIndex < 0) return;
    const itemEl = listboxRef.current.children[highlightedIndex] as HTMLElement;
    itemEl?.scrollIntoView({ block: "nearest" });
  }, [highlightedIndex]);

  // Click outside listener
  useEffect(() => {
    function handlePointerDown(e: PointerEvent) {
      if (!isOpen || !rootRef.current) return;
      if (!rootRef.current.contains(e.target as Node)) {
        closeDropdown();
      }
    }

    function handleResizeOrScroll() {
      if (isOpen) calculatePlacement();
    }

    window.addEventListener("pointerdown", handlePointerDown, { capture: true });
    window.addEventListener("resize", handleResizeOrScroll, { passive: true });
    window.addEventListener("scroll", handleResizeOrScroll, { passive: true, capture: true });

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, { capture: true });
      window.removeEventListener("resize", handleResizeOrScroll);
      window.removeEventListener("scroll", handleResizeOrScroll, { capture: true });
    };
  }, [isOpen, closeDropdown, calculatePlacement]);

  // WAI-ARIA Keyboard Navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (disabled) return;

    if (!isOpen) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        openDropdown();
      }
      return;
    }

    switch (e.key) {
      case "Escape":
        e.preventDefault();
        closeDropdown();
        triggerRef.current?.focus();
        break;

      case "Tab":
        closeDropdown();
        break;

      case "ArrowDown": {
        e.preventDefault();
        let next = highlightedIndex + 1;
        while (next < items.length && items[next].disabled) next++;
        if (next < items.length) setHighlightedIndex(next);
        break;
      }

      case "ArrowUp": {
        e.preventDefault();
        let prev = highlightedIndex - 1;
        while (prev >= 0 && items[prev].disabled) prev--;
        if (prev >= 0) setHighlightedIndex(prev);
        break;
      }

      case "Home": {
        e.preventDefault();
        const first = items.findIndex((it) => !it.disabled);
        if (first >= 0) setHighlightedIndex(first);
        break;
      }

      case "End": {
        e.preventDefault();
        let last = items.length - 1;
        while (last >= 0 && items[last].disabled) last--;
        if (last >= 0) setHighlightedIndex(last);
        break;
      }

      case "Enter":
      case " ": {
        e.preventDefault();
        if (highlightedIndex >= 0 && highlightedIndex < items.length) {
          const opt = items[highlightedIndex];
          if (!opt.disabled) selectOption(opt);
        }
        break;
      }

      default: {
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          e.preventDefault();
          if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);
          searchBufferRef.current += e.key.toLowerCase();
          searchTimeoutRef.current = setTimeout(() => {
            searchBufferRef.current = "";
          }, 600);

          const q = searchBufferRef.current;
          const match = items.findIndex(
            (it) => !it.disabled && it.label.toLowerCase().startsWith(q)
          );
          if (match >= 0) setHighlightedIndex(match);
        }
        break;
      }
    }
  };

  const SelectedIcon = selectedItem?.icon;

  return (
    <div
      ref={rootRef}
      className={`relative inline-block w-full select-none font-sans ${className}`}
      onKeyDown={handleKeyDown}
    >
      {name && <input type="hidden" name={name} value={value} />}

      {/* Button Trigger */}
      <button
        ref={triggerRef}
        type="button"
        id={instanceId}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={listboxId}
        aria-label={ariaLabel || placeholder}
        aria-activedescendant={
          isOpen && highlightedIndex >= 0 ? `${instanceId}-opt-${highlightedIndex}` : undefined
        }
        disabled={disabled}
        onClick={toggleDropdown}
        className={`
          w-full flex items-center justify-between
          bg-[var(--select-bg,#121217)] text-[var(--select-text-primary,#ece8ff)]
          border border-[var(--select-border,rgba(255,255,255,0.08))]
          shadow-[var(--select-hairline,inset_0_1px_0_0_rgba(255,255,255,0.08))]
          hover:border-[var(--select-border-hover,rgba(255,255,255,0.18))]
          hover:bg-[var(--select-surface,rgba(28,28,36,0.78))]
          active:scale-[0.98] transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)]
          outline-none cursor-pointer
          ${glowOnActive && isOpen ? "border-[var(--accent-color,#3d5afe)] ring-2 ring-[var(--accent-glow,rgba(61,90,254,0.28))]" : ""}
          focus-visible:border-[var(--accent-color,#3d5afe)] focus-visible:ring-2 focus-visible:ring-[var(--accent-glow,rgba(61,90,254,0.28))]
          disabled:opacity-40 disabled:cursor-not-allowed disabled:transform-none
          ${sizeClasses.trigger}
        `}
      >
        <div className="flex items-center gap-2 min-w-0 flex-1 text-left">
          {SelectedIcon && (
            <span className="text-[var(--select-text-secondary,#a1a1aa)] shrink-0">
              <SelectedIcon size={sizeClasses.iconSize} />
            </span>
          )}
          <span
            className={`truncate font-medium ${
              !selectedItem ? "text-[var(--select-text-muted,#71717a)] font-normal" : ""
            }`}
          >
            {selectedItem ? selectedItem.label : placeholder}
          </span>
        </div>

        {/* Minimalist 1.5px Chevron with physics-based 180° rotation */}
        <span
          className={`shrink-0 text-[var(--select-text-secondary,#a1a1aa)] transition-transform duration-260 ease-[cubic-bezier(0.16,1,0.3,1)] ${
            isOpen ? "rotate-180 text-[var(--accent-color,#3d5afe)]" : ""
          }`}
        >
          <ChevronDown size={sizeClasses.iconSize} strokeWidth={1.5} />
        </span>
      </button>

      {/* Flyout Menu (Popover) */}
      {isOpen && (
        <div
          className={`
            absolute left-0 right-0 z-50 min-w-full
            ${placement === "bottom" ? "top-[calc(100%+6px)] origin-top animate-iris-in-bottom" : "bottom-[calc(100%+6px)] origin-bottom animate-iris-in-top"}
          `}
          role="presentation"
        >
          <ul
            ref={listboxRef}
            id={listboxId}
            role="listbox"
            tabIndex={-1}
            aria-label={ariaLabel || placeholder}
            className="
              m-0 p-1 list-none
              bg-[var(--select-surface,rgba(28,28,36,0.88))]
              backdrop-blur-xl saturate-180
              border border-[var(--select-border,rgba(255,255,255,0.08))]
              rounded-[13px]
              shadow-[var(--select-shadow-flyout)]
              max-h-[280px] overflow-y-auto overscroll-contain outline-none
            "
          >
            {items.map((item, index) => {
              const isSelected = item.value === value;
              const isHighlighted = index === highlightedIndex;
              const OptionIcon = item.icon;

              return (
                <li
                  key={String(item.value)}
                  id={`${instanceId}-opt-${index}`}
                  role="option"
                  aria-selected={isSelected}
                  aria-disabled={item.disabled}
                  onPointerEnter={() => {
                    if (!item.disabled) setHighlightedIndex(index);
                  }}
                  onClick={() => selectOption(item)}
                  className={`
                    relative flex items-center justify-between gap-2.5 rounded-lg cursor-pointer
                    transition-colors duration-140 ease-out
                    ${item.disabled ? "opacity-40 cursor-not-allowed" : ""}
                    ${isSelected ? "text-[var(--accent-color,#3d5afe)] font-semibold" : "text-[var(--select-text-primary,#ece8ff)]"}
                    ${sizeClasses.option}
                  `}
                >
                  {/* Floating spotlight pill background */}
                  <span
                    className={`
                      absolute inset-0 rounded-lg pointer-events-none transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)]
                      ${isSelected ? "bg-[var(--accent-subtle,rgba(61,90,254,0.12))]" : ""}
                      ${isHighlighted && !isSelected ? "bg-[var(--select-surface-hover,rgba(255,255,255,0.06))]" : ""}
                      ${isHighlighted && isSelected ? "bg-[var(--accent-subtle-hover,rgba(61,90,254,0.18))]" : ""}
                    `}
                  />

                  {/* Left content: Icon + Label & Subtitle */}
                  <div className="relative z-10 flex items-center gap-2 min-w-0 flex-1">
                    {OptionIcon && (
                      <span
                        className={`shrink-0 ${
                          isSelected
                            ? "text-[var(--accent-color,#3d5afe)]"
                            : "text-[var(--select-text-secondary,#a1a1aa)]"
                        }`}
                      >
                        <OptionIcon size={sizeClasses.iconSize} />
                      </span>
                    )}

                    <div className="flex flex-col min-w-0 text-left">
                      <span className="truncate leading-tight">{item.label}</span>
                      {item.description && (
                        <span className="truncate text-xs text-[var(--select-text-muted,#71717a)] font-normal leading-tight mt-0.5">
                          {item.description}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Right content: Shortcut badge + Animated checkmark */}
                  <div className="relative z-10 flex items-center gap-2 shrink-0">
                    {item.shortcut && (
                      <kbd className="font-mono text-[10px] text-[var(--select-text-muted,#71717a)] bg-white/5 border border-white/10 rounded px-1.5 py-0.5">
                        {item.shortcut}
                      </kbd>
                    )}

                    <span
                      className={`
                        inline-flex items-center justify-center w-4 h-4 text-[var(--accent-color,#3d5afe)]
                        transition-all duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]
                        ${isSelected ? "opacity-100 scale-100" : "opacity-0 scale-75"}
                      `}
                    >
                      <Check size={14} strokeWidth={2} />
                    </span>
                  </div>
                </li>
              );
            })}

            {items.length === 0 && (
              <li className="p-4 text-center text-xs text-[var(--select-text-muted,#71717a)]">
                Sin opciones disponibles
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
