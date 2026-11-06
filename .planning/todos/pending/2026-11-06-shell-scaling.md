# Independent text and shell sizing

Backlog: independent text size and whole-shell scale in `docs/BACKLOG.md`.

`shell.fonts.base_size` controls typography relative to the default 14 logical
pixels. A separate profile-scoped shell scale controls authored absolute layout
lengths and scales the resulting text along with the rest of the shell. The
compositor output scale remains independent. Both settings need localized,
accessible controls, reset actions, validation and transactional persistence.

The theme snapshot and style resolution must carry both factors. Apply each
factor once, including inherited text, control parts, keyframes and repeated
restyles. Font-size changes require text remeasurement and layout; shell-scale
changes additionally affect surface dimensions, icons, spacing and effects.
Ensure initial loading, theme changes and profile switching apply the same
settings as live changes. Define the default of the new shell-scale setting as
100%; existing base-size preferences remain text preferences rather than being
silently copied into a second setting.

Verify independent and combined changes, inheritance and reset, profile
precedence and validation, Settings interaction, surface and popup geometry,
accessibility and output scaling. Recheck the appearance-switch measurements
and live slider behavior tracked separately in the backlog.
