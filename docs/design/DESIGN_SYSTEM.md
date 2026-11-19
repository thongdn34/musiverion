# Design system — Blue Glass

Frosted glass panels floating over a deep blue gradient. Dark is the primary theme. Mockups: `mockups.html`.

## Principles
1. **Glass has depth, not decoration.** Three levels only: backdrop → panel → raised (dialogs, selected rows, popovers). Never stack more than two glass layers.
2. **Blue carries meaning sparingly.** The bright blue gradient is reserved for the one primary action per screen and for "B / current" in comparisons.
3. **Audio is the content.** Waveforms get the highest contrast on the screen; chrome stays quiet.
4. **Readable first.** Body text on glass must reach 4.5:1 contrast; if a panel sits over a bright part of the backdrop, raise its tint rather than lowering the text weight.

## Tokens (CSS custom properties, defined in `packages/ui/src/tokens.css`)

| Token | Value | Use |
|---|---|---|
| `--bg-0` | `#050f26` | backdrop base |
| `--bg-1` | `#0a2463` | backdrop mid |
| `--bg-2` | `#0e4ec4` | backdrop highlight |
| `--orb-a` / `--orb-b` | `#3b82f6` / `#22d3ee` | blurred backdrop orbs (opacity .45 / .28, blur 60px) |
| `--glass-1` | `rgba(255,255,255,.07)` | panel fill |
| `--glass-2` | `rgba(255,255,255,.13)` | raised fill, selected row |
| `--glass-border` | `rgba(255,255,255,.16)` | 1px panel border |
| `--glass-highlight` | `inset 0 1px 0 rgba(255,255,255,.22)` | top edge light |
| `--blur` | `24px` (+ `saturate(140%)`) | `backdrop-filter` on panels |
| `--shadow` | `0 12px 40px rgba(2,8,30,.45)` | raised level only |
| `--text` | `#f3f7ff` | primary text |
| `--text-2` | `rgba(222,233,255,.68)` | secondary text |
| `--text-3` | `rgba(222,233,255,.45)` | labels, disabled |
| `--primary` | `linear-gradient(135deg,#2f7bff,#56c2ff)` | primary button, B marker |
| `--primary-solid` | `#4c9bff` | focus ring, links, waveform B |
| `--wave-a` | `#b9c8ee` | waveform A (reference) |
| `--diff` | `#ffb84d` | changed regions, unsaved state |
| `--ok` | `#4ade9a` | approved, connected |
| `--danger` | `#ff7a90` | conflicts, destructive |
| `--radius-s/m/l` | `8 / 14 / 20px` | controls / panels / windows, phone cards |
| `--space` | 4px grid: 4, 8, 12, 16, 24, 32 | all spacing |

## Type
- UI: Inter (fallback system-ui). 13px base on desktop, 15px on phone. Weights 400 / 600 only.
- Labels: 11px, uppercase, +0.06em, `--text-3`.
- Numbers and change lists: JetBrains Mono (fallback ui-monospace), tabular figures.

## Components
- **Panel:** `--glass-1`, border, highlight, blur, `--radius-m`. No shadow.
- **Raised:** `--glass-2`, border, highlight, blur, shadow. Dialogs dim the window with `rgba(3,10,30,.55)`.
- **Button:** 32px high (44px on phone), `--radius-s`. Primary = gradient with white text; secondary = `--glass-2`; ghost = text only. One primary per screen.
- **Chip:** pill, 11px. Neutral = glass; state chips use the state colour at 18% fill with the colour as text.
- **Row:** 8px radius; hover `--glass-1`, selected `--glass-2` plus a 2px `--primary-solid` left edge.
- **Waveform:** bars, 2px wide, 1px gap, on a `rgba(0,0,0,.22)` well. A = `--wave-a`, B = `--primary-solid`, changed regions = `--diff` at 22% fill with 1px edges.
- **Progress:** 6px track `--glass-2`, fill `--primary`.
- **Focus:** 2px `--primary-solid` ring, 2px offset. Always visible for keyboard users.

## Motion
150 ms ease-out for hover and selection; 220 ms for dialogs (fade + 8px rise). Respect `prefers-reduced-motion`.

## Accessibility and performance rules
- If `prefers-reduced-transparency` is set, or `backdrop-filter` is unsupported (some Linux WebKitGTK builds), swap glass fills for solid `#12244f` / `#1a3168`.
- State is never colour-only: every chip has a word, every changed region has a time range in text.
- Blur only on panels, never on scrolling list rows.

## Light theme
Deferred to Phase 6. Tokens are named by role so a light set can be added without touching components.
