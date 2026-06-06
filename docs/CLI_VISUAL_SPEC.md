# CLI Visual Spec

This document records the implementation-facing visual contract for the
`deepseek-shell` TUI theme. It is the stable reference for colors, symbols, and
fallback behavior used by the CLI visual redesign.

## Theme

- Theme id: `deepseek-shell`
- User command: `/theme deepseek-shell`
- Settings value: `theme = "deepseek-shell"` in `settings.toml`
- Aliases: `deepseek`, `ds-shell`, `shell`
- Default behavior: the theme is opt-in and does not replace `system`

## Color Tokens

Core colors intentionally use ANSI 256 indexes so the style survives common
terminal profiles without relying on 24-bit truecolor.

| Role | Token | Value |
| --- | --- | --- |
| Root background | `surface_bg` | `Indexed(233)` |
| Header background | `header_bg` | `Indexed(233)` |
| Footer background | `footer_bg` | `Indexed(233)` |
| Panel / AI surface | `panel_bg` | `Indexed(236)` |
| Elevated surface | `elevated_bg` | `Indexed(237)` |
| Composer background | `composer_bg` | `Indexed(236)` |
| Selection | `selection_bg` | `Indexed(24)` |
| Border | `border` | `Indexed(238)` |
| Primary text | `text_body` | `Indexed(255)` |
| Soft text | `text_soft` | `White` |
| Muted text | `text_muted` | `Indexed(245)` |
| Hint text | `text_hint` | `Indexed(243)` |
| Dim text | `text_dim` | `Indexed(240)` |
| Primary accent / user/action | `accent_primary`, `accent_action` | `Indexed(69)` |
| Secondary accent / system/thinking | `accent_secondary`, `status_working` | `Indexed(99)` |
| Warning | `status_warning`, `warning` | `Yellow` |
| Success | `success`, `diff_added_fg` | `Green` |
| Error | `error_fg`, `error_text`, `tool_failed` | `LightRed` |
| Error surface | `error_surface`, `diff_deleted_bg` | `Indexed(52)` |
| Diff added background | `diff_added_bg` | `Indexed(22)` |
| Tool running | `tool_running` | `Indexed(69)` |
| Tool success | `tool_success` | `Indexed(245)` |

## Information Hierarchy

- User messages use the primary blue emphasis.
- AI/body text stays high contrast on quiet dark surfaces.
- System, thinking, and live status signals use the purple secondary accent.
- Errors and warnings use semantic colors, not accent colors.
- Secondary telemetry such as cost, cache, worked time, and inactive MCP state
  uses muted text.
- Footer, header, toast, retry, diff-preview, inline diff, tool-card, and status colors
  must come from the active `UiTheme`, not from hard-coded default palette
  constants.
- Transcript, Markdown, transcript copy metadata, file-tree, and sidebar chrome
  must derive their active render theme from the runtime `UiTheme` so custom
  backgrounds and community theme tokens survive the render path.
- Full-frame rendering must scope `active_theme()` to the runtime `UiTheme` so
  non-transcript helper paths inherit the selected theme.

## Symbol Tokens

| Surface | Unicode | ASCII fallback |
| --- | --- | --- |
| Composer prompt | `❯ ` | `> ` |
| Empty-state brand | `❯ codewhale` | `> codewhale` |
| Header status indicator | whale/dot animation | `.`, `..`, `...` |
| Footer live strip | block-height wave | `-`, `=`, `~` |
| Retry active | `⟳ retry N in Ss — reason` | `retry N in Ss - reason` |
| Retry failed | `× failed: reason` | `failed: reason` |
| Compacting / purging | `compacting ⎋`, `purging ⎋` | `compacting`, `purging` |
| Context seam status | `⏻ producing...`, `⏻ L... seam complete` | `* producing...`, `* L... seam complete` |
| Shell running chip | `⏳ shell running` | `> shell running` |
| User role glyph | `▎` | `>` |
| Assistant role glyph | `●` | `*` |
| Transcript rail | `▏ ` | `| ` |
| Thinking opener | `…` | `...` |
| Thinking rail | `╎ ` | `: ` |
| Thinking cursor | `▎` | `|` |
| Tool running status | `·`, `◦`, `•`, `◦` | `.`, `:`, `*`, `:` |
| Tool done / failed status | `•` | `+`, `!` |
| Plan prompt step status | `·`, `▶`, `✓` | `.`, `>`, `+` |
| Update-plan result status | `○`, `◎`, `●` | `o`, `>`, `+` |
| Pending input section | `•`, `↳`, `▸`, `…`, `·`, `↑` | `-`, `>`, `...`, ` - `, `Up` |
| Picker selected row | `▸`, `▶` | `>` |
| Onboarding language mark | `●`, `○` | `*`, `o` |
| Fanout worker grid | `●`, `◐`, `×`, `⊘`, `○` | `#`, `~`, `x`, `!`, `o` |

Task status glyphs:

| Surface | Unicode | ASCII fallback |
| --- | --- | --- |
| Task list/detail status | `U+25CB`, `U+25D0`, `U+2713`, `U+2717`, `U+2298` | `o`, `>`, `+`, `x`, `-` |
| Task detail tool-call status | `U+25D0`, `U+2713`, `U+2717`, `U+2298` | `>`, `+`, `x`, `-` |
| Tasks sidebar background status | `[U+25CB]`, `[U+25D0]`, `[U+2713]`, `[U+2717]`, `[U+2298]` | `[ ]`, `[~]`, `[x]`, `[!]`, `[-]` |

Tool-card family glyphs:

| Family | Unicode | ASCII fallback |
| --- | --- | --- |
| Read | `▷` | `>` |
| Patch | `◆` | `*` |
| Run | `▶` | `>` |
| Find | `⌕` | `?` |
| Delegate | `◐` | `@` |
| Fanout | `⋮⋮` | `>>` |
| RLM | `⋮⋮` | `::` |
| Plan | `◇` | `=` |
| Verify | `✓` | `+` |
| Think | `…` | `...` |
| Generic | `•` | `*` |

`Plan` covers `update_plan`/strategy update cards. `Verify` covers explicit
verifier/test tools and review output cards.

## Fallback Rules

- `CODEWHALE_ASCII_UI=1` enables plain-ASCII UI symbols for terminals or fonts
  that cannot render Unicode reliably.
- Truthy values are `1`, `true`, `yes`, `on`, and `ascii`.
- ANSI 256 colors are preferred for `deepseek-shell`.
- ANSI 16 terminals downgrade indexed foreground colors to the nearest named
  color.
- ANSI 16 backgrounds drop back to terminal reset backgrounds where needed to
  avoid unreadable tinted blocks.
- Unicode symbols must never be the only source of meaning; labels and colors
  must still communicate status when ASCII fallback is active.
- Command text ASCII fallback must cover structural symbols and status markers:
  rules, bullets, arrows, checkmarks, crosses, warning signs, rails, picker
  pointers, prompt chevrons, submit/Alt key glyphs, thinking rails, tool-card
  family glyphs, checklist boxes, skill markers, fanout worker symbols,
  compacting escape markers, context seam status markers, status-indicator
  symbols, and sidebar progress blocks.
- Slash command result messages and command errors must pass through the same
  command text fallback at the `CommandResult` boundary, so individual command
  handlers do not need to remember to normalize every decorative symbol.
- Transcript system and categorized error cells must pass through command text
  fallback at history insertion boundaries; user, assistant, and tool content
  stay unmodified.
- Low-motion and no-animation modes must remain compatible with the theme:
  `NO_ANIMATIONS=1`, `low_motion = true`, `fancy_animations = false`,
  `status_indicator = "off"`, and `synchronized_output = "off"` should reduce
  motion or flicker without hiding status text.

## Verification Checklist

- `/theme deepseek-shell` switches and persists the theme.
- `/theme` picker lists `DeepSeek Shell` and shows its swatch.
- `/theme` picker previews keep the active `background_color` overlay when
  moving across candidate themes.
- `/theme` picker selected rows use `selection_text` on `selection_bg` for
  their pointer, number, name, and tagline.
- Composer cursor and wrapping account for the prompt width.
- DeepSeek Shell empty-state branding uses the same terminal prompt language
  as the composer.
- Header, footer, toast stack, command palette, help/session picker,
  plan/user-input modals, file/model/provider and mode/status/feedback pickers,
  onboarding screens, history, pending-input preview, file-tree/sidebar chrome,
  approval/elevation modals, config/shell-control/sub-agents modals,
  context menu, pager/live transcript overlays,
  terminal title animation/completion titles,
  setup/status/doctor/init/session-fork CLI output,
  PR review composer prompts,
  config/shell-control modals, sub-agent/fanout cards, and tool cards use
  active theme colors.
- User, AI, system, tool, warning, and error states are visually distinct.
- `CODEWHALE_ASCII_UI=1` keeps composer, empty-state brand line, composer panel borders, composer
  menus, composer submit hints, header, transcript, thinking placeholders, cycle-boundary rules/arrows, footer, retry,
  shell, tool, footer balance currency,
  command text output rules/headings/separators/bullets/ellipsis/missing markers,
  slash-command result messages/errors,
  transcript system/error messages,
  context seam status markers,
  read-file numbered output separators,
  edit-file fuzzy-match summary separators,
  restore snapshot-label ellipses,
  task-list/task-detail and Tasks sidebar status markers,
  update-plan tool status symbols,
  web-search API error separators,
  client stream error separators,
  pandoc conversion summaries,
  large-output truncation notes,
  `/settings` display separators,
  localized `/tokens` and `/cost` report rules,
  localized `/model` change arrows,
  localized width truncation ellipsis,
  API-key help bullets,
  status/change summaries, warning prefixes, picker navigation hints,
  jump-to-latest, theme-picker, file-tree, pending-input separators, modal
  summary separators,
  command-palette/theme-picker/file-picker/model-picker/provider-picker/session-picker/mode-picker/status-picker/feedback-picker/plan-prompt/user-input/context-menu/pager/help-overlay borders,
  setup/status/doctor/init/session-fork CLI markers/arrows,
  transient receipt, status-message, and status-toast markers/separators,
  PR review prompt separators/arrows/ellipsis,
  command-palette description separators,
  large-paste toast separators,
  onboarding panel, config/shell-control/sub-agents modal chrome,
  sub-agents steps marker,
  modal/approval/elevation borders, title separators, and left rails,
  help-overlay truncation/keybinding chords/chrome, pager chrome,
  live-transcript chrome/cache render context, chat
  scrollbar, live-transcript backtrack markers, sidebar status markers/progress bars,
  sidebar context missing-value placeholders,
  checklist status markers, sidebar resize handle, tool-card rails,
  agent-card rails, fanout dots/count separators, header inline separators, command-menu
  truncation, key-hint arrows/Alt prefix, Markdown table/rule chrome,
  terminal title animation/completion titles, spillover pager titles,
  sandbox-denied titles, hook block reasons, slop-ledger export headings,
  completion-gate titles and truncation markers, RLM code summaries, plugin
  override descriptions, skill prompt truncation markers, MCP error body
  excerpts, and decision-card chrome/glyphs ASCII.
- `NO_ANIMATIONS=1` and the equivalent low-motion settings keep header,
  footer, terminal-title, thinking, and streaming states readable with passive
  animation disabled.
- Agent-card action and summary rows must truncate against the current render
  width using display width so CJK, emoji, and other wide characters do not
  overflow their visual budget.
- Tool-card headers must truncate command, path, query, and argument summaries
  against the current render width using display width; long command/path
  details remain available in the wrapped body rows or transcript view.
- Tool-card family mapping must cover the registered built-in tool surface, not
  only legacy `read_file`/`exec_shell` names: checklist/todo, task,
  automation, Git/GitHub, web/finance, RLM, sub-agent, verifier, and
  large-result retrieval tools should render with semantic read/patch/run/find/
  delegate/fanout/rlm/plan/verify labels instead of the generic fallback.
- Plan-prompt explanation text must wrap against terminal display width so CJK
  and other wide characters do not overflow modal content.
- Modal ASCII chrome titles must truncate against display width so localized
  titles cannot run over narrow borders.
- Picker ASCII chrome titles and footer hints must also truncate against
  display width; localized picker text cannot overflow plain borders.
- File-picker candidate paths must truncate against display width while
  preserving the useful filename suffix for long or localized paths.
- Config and sub-agent table text must truncate and pad against display width
  so localized keys, values, and summaries keep columns aligned.
- Fanout worker dot grids must also truncate against the current render width;
  aggregate counts remain visible on the status row.
- 80-column layout remains readable without wrapping status chips into a second
  row.
- Render coverage should include DeepSeek Shell header, footer, and composer at
  80 columns, plus ASCII fallback tests for truncation markers and structural
  separators.

## Verification Notes

Static checks should confirm:

- `ThemeId::DeepSeekShell`, `DEEPSEEK_SHELL_UI_THEME`, and
  `SELECTABLE_THEMES` expose the opt-in theme without changing the default
  `system` behavior.
- `/theme deepseek-shell` and aliases normalize to the canonical
  `deepseek-shell` settings value and preserve runtime `background_color`
  overlays.
- Header, footer, composer, picker, modal, history, tool-card, sidebar,
  terminal-title, and command-output paths use active theme tokens or documented
  ASCII fallback helpers.
- CJK/wide-character truncation and padding helpers use display width, not byte
  or scalar counts, on fixed-width rows.

Runtime release validation still requires:

- `cargo test` for the affected TUI/theme modules.
- Snapshot or render review of `/theme deepseek-shell`, `/theme system`, and
  `CODEWHALE_ASCII_UI=1`.
- Manual dogfood in an 80-column terminal and a wider terminal, including
  low-motion settings and at least one tool-running transcript.
