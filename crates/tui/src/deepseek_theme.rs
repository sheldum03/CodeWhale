//! Whale/DeepSeek terminal theme tokens.
//!
//! This module is the compact bridge between the broad [`crate::palette::UiTheme`]
//! model and render paths that need named semantic tokens for transcript,
//! sidebar, plan, diff, and tool chrome. Keep visual decisions here when a
//! surface needs role-level colors or styles instead of raw palette constants;
//! that keeps the opt-in `deepseek-shell` theme, community themes, and runtime
//! background overrides moving through the same render contract.

use std::cell::Cell;

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{BorderType, Borders, Padding};

use crate::palette;
use crate::palette::PaletteMode;
use crate::palette::ThemeId;
use crate::palette::UiTheme;
use crate::tui::history::ToolStatus;

/// Visual variant exposed by the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Dark,
    DeepSeekShell,
    Light,
    Grayscale,
}

thread_local! {
    static ACTIVE_THEME_OVERRIDE: Cell<Option<Theme>> = Cell::new(None);
}

/// Centralized visual tokens for sidebar, plan, and tool rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub variant: Variant,

    // Sidebar / section chrome
    pub section_borders: Borders,
    pub section_border_type: BorderType,
    pub section_border_color: Color,
    pub section_bg: Color,
    pub section_title_color: Color,
    pub section_padding: Padding,

    // Transcript/message color tokens
    pub text_body_color: Color,
    pub text_muted_color: Color,
    pub text_dim_color: Color,
    pub text_soft_color: Color,
    pub user_accent_color: Color,
    pub user_message_bg: Color,
    pub assistant_accent_color: Color,
    pub reasoning_text_color: Color,
    pub reasoning_live_color: Color,
    pub status_success_color: Color,
    pub status_warning_color: Color,
    pub status_error_color: Color,
    pub status_info_color: Color,

    // Diff preview color tokens
    pub diff_added_fg: Color,
    pub diff_deleted_fg: Color,
    pub diff_added_bg: Color,
    pub diff_deleted_bg: Color,

    // Tool cell color tokens
    pub tool_title_color: Color,
    pub tool_value_color: Color,
    pub tool_label_color: Color,
    pub tool_running_accent: Color,
    pub tool_success_accent: Color,
    pub tool_failed_accent: Color,

    // Plan cell color tokens
    pub plan_progress_color: Color,
    pub plan_summary_color: Color,
    pub plan_explanation_color: Color,
    pub plan_pending_color: Color,
    pub plan_in_progress_color: Color,
    pub plan_completed_color: Color,
}

impl Theme {
    /// Background used to stretch user-message rows across the transcript.
    #[must_use]
    pub const fn user_message_bg_color(self) -> Color {
        self.user_message_bg
    }

    /// The current dark theme. Visible output today uses these values.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            variant: Variant::Dark,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: palette::BORDER_COLOR,
            section_bg: palette::DEEPSEEK_INK,
            section_title_color: palette::DEEPSEEK_BLUE,
            // Horizontal padding only. `Padding::uniform(1)` ate two rows of
            // each sidebar panel — for compact terminals where Work/Tasks/Agents
            // get ~3 rows total via the 25% layout split, that left zero rows
            // for content (#63 follow-up: panels rendered as empty boxes even
            // when "No todos" / "No active plan" should have shown).
            section_padding: Padding::horizontal(1),
            text_body_color: palette::TEXT_PRIMARY,
            text_muted_color: palette::TEXT_MUTED,
            text_dim_color: palette::TEXT_DIM,
            text_soft_color: palette::TEXT_SOFT,
            user_accent_color: palette::USER_BODY,
            user_message_bg: palette::USER_MESSAGE_BG,
            assistant_accent_color: palette::DEEPSEEK_SKY,
            reasoning_text_color: palette::TEXT_REASONING,
            reasoning_live_color: palette::ACCENT_REASONING_LIVE,
            status_success_color: palette::STATUS_SUCCESS,
            status_warning_color: palette::STATUS_WARNING,
            status_error_color: palette::STATUS_ERROR,
            status_info_color: palette::STATUS_INFO,
            diff_added_fg: palette::UI_THEME.diff_added_fg,
            diff_deleted_fg: palette::UI_THEME.diff_deleted_fg,
            diff_added_bg: palette::UI_THEME.diff_added_bg,
            diff_deleted_bg: palette::UI_THEME.diff_deleted_bg,
            tool_title_color: palette::TEXT_SOFT,
            tool_value_color: palette::TEXT_MUTED,
            tool_label_color: palette::TEXT_DIM,
            tool_running_accent: palette::ACCENT_TOOL_LIVE,
            tool_success_accent: palette::TEXT_DIM,
            tool_failed_accent: palette::ACCENT_TOOL_ISSUE,
            plan_progress_color: palette::STATUS_SUCCESS,
            plan_summary_color: palette::TEXT_MUTED,
            plan_explanation_color: palette::TEXT_DIM,
            plan_pending_color: palette::TEXT_MUTED,
            plan_in_progress_color: palette::STATUS_WARNING,
            plan_completed_color: palette::STATUS_SUCCESS,
        }
    }

    /// DeepSeek Shell tokens for terminal-first visual redesign work.
    #[must_use]
    pub const fn deepseek_shell() -> Self {
        Self {
            variant: Variant::DeepSeekShell,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: palette::DEEPSEEK_SHELL_UI_THEME.border,
            section_bg: palette::DEEPSEEK_SHELL_UI_THEME.surface_bg,
            section_title_color: palette::DEEPSEEK_SHELL_UI_THEME.accent_primary,
            section_padding: Padding::horizontal(1),
            text_body_color: palette::DEEPSEEK_SHELL_UI_THEME.text_body,
            text_muted_color: palette::DEEPSEEK_SHELL_UI_THEME.text_muted,
            text_dim_color: palette::DEEPSEEK_SHELL_UI_THEME.text_dim,
            text_soft_color: palette::DEEPSEEK_SHELL_UI_THEME.text_soft,
            user_accent_color: palette::DEEPSEEK_SHELL_UI_THEME.accent_primary,
            user_message_bg: palette::DEEPSEEK_SHELL_UI_THEME.elevated_bg,
            assistant_accent_color: palette::DEEPSEEK_SHELL_UI_THEME.status_working,
            reasoning_text_color: palette::DEEPSEEK_SHELL_UI_THEME.accent_secondary,
            reasoning_live_color: palette::DEEPSEEK_SHELL_UI_THEME.accent_secondary,
            status_success_color: palette::DEEPSEEK_SHELL_UI_THEME.success,
            status_warning_color: palette::DEEPSEEK_SHELL_UI_THEME.status_warning,
            status_error_color: palette::DEEPSEEK_SHELL_UI_THEME.error_fg,
            status_info_color: palette::DEEPSEEK_SHELL_UI_THEME.info,
            diff_added_fg: palette::DEEPSEEK_SHELL_UI_THEME.diff_added_fg,
            diff_deleted_fg: palette::DEEPSEEK_SHELL_UI_THEME.diff_deleted_fg,
            diff_added_bg: palette::DEEPSEEK_SHELL_UI_THEME.diff_added_bg,
            diff_deleted_bg: palette::DEEPSEEK_SHELL_UI_THEME.diff_deleted_bg,
            tool_title_color: palette::DEEPSEEK_SHELL_UI_THEME.text_soft,
            tool_value_color: palette::DEEPSEEK_SHELL_UI_THEME.text_muted,
            tool_label_color: palette::DEEPSEEK_SHELL_UI_THEME.text_hint,
            tool_running_accent: palette::DEEPSEEK_SHELL_UI_THEME.tool_running,
            tool_success_accent: palette::DEEPSEEK_SHELL_UI_THEME.tool_success,
            tool_failed_accent: palette::DEEPSEEK_SHELL_UI_THEME.tool_failed,
            plan_progress_color: palette::DEEPSEEK_SHELL_UI_THEME.status_working,
            plan_summary_color: palette::DEEPSEEK_SHELL_UI_THEME.text_muted,
            plan_explanation_color: palette::DEEPSEEK_SHELL_UI_THEME.text_hint,
            plan_pending_color: palette::DEEPSEEK_SHELL_UI_THEME.text_muted,
            plan_in_progress_color: palette::DEEPSEEK_SHELL_UI_THEME.warning,
            plan_completed_color: palette::DEEPSEEK_SHELL_UI_THEME.success,
        }
    }

    /// Light theme tokens for sidebar and tool chrome.
    #[must_use]
    pub const fn light() -> Self {
        Self {
            variant: Variant::Light,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: palette::LIGHT_BORDER,
            section_bg: palette::LIGHT_PANEL,
            section_title_color: palette::DEEPSEEK_BLUE,
            section_padding: Padding::horizontal(1),
            text_body_color: palette::LIGHT_TEXT_BODY,
            text_muted_color: palette::LIGHT_TEXT_MUTED,
            text_dim_color: palette::LIGHT_TEXT_HINT,
            text_soft_color: palette::LIGHT_TEXT_SOFT,
            user_accent_color: palette::DEEPSEEK_BLUE,
            user_message_bg: palette::LIGHT_UI_THEME.elevated_bg,
            assistant_accent_color: palette::DEEPSEEK_BLUE,
            reasoning_text_color: Color::Rgb(180, 83, 9),
            reasoning_live_color: Color::Rgb(180, 83, 9),
            status_success_color: palette::DEEPSEEK_BLUE,
            status_warning_color: Color::Rgb(180, 83, 9),
            status_error_color: palette::DEEPSEEK_RED,
            status_info_color: palette::DEEPSEEK_BLUE,
            diff_added_fg: palette::LIGHT_UI_THEME.diff_added_fg,
            diff_deleted_fg: palette::LIGHT_UI_THEME.diff_deleted_fg,
            diff_added_bg: palette::LIGHT_UI_THEME.diff_added_bg,
            diff_deleted_bg: palette::LIGHT_UI_THEME.diff_deleted_bg,
            tool_title_color: palette::LIGHT_TEXT_SOFT,
            tool_value_color: palette::LIGHT_TEXT_MUTED,
            tool_label_color: palette::LIGHT_TEXT_HINT,
            tool_running_accent: palette::DEEPSEEK_BLUE,
            tool_success_accent: palette::LIGHT_TEXT_HINT,
            tool_failed_accent: palette::DEEPSEEK_RED,
            plan_progress_color: palette::DEEPSEEK_BLUE,
            plan_summary_color: palette::LIGHT_TEXT_MUTED,
            plan_explanation_color: palette::LIGHT_TEXT_HINT,
            plan_pending_color: palette::LIGHT_TEXT_MUTED,
            plan_in_progress_color: Color::Rgb(180, 83, 9),
            plan_completed_color: palette::DEEPSEEK_BLUE,
        }
    }

    /// Solarized Light theme tokens — warm ivory tones, high contrast.
    #[must_use]
    pub const fn solarized_light() -> Self {
        Self {
            variant: Variant::Light,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: palette::SOLARIZED_BORDER,
            section_bg: palette::SOLARIZED_PANEL,
            section_title_color: palette::SOLARIZED_BLUE,
            section_padding: Padding::horizontal(1),
            text_body_color: palette::SOLARIZED_TEXT_BODY,
            text_muted_color: palette::SOLARIZED_TEXT_MUTED,
            text_dim_color: palette::SOLARIZED_TEXT_DIM,
            text_soft_color: palette::SOLARIZED_TEXT_SOFT,
            user_accent_color: palette::SOLARIZED_BLUE,
            user_message_bg: palette::SOLARIZED_LIGHT_UI_THEME.elevated_bg,
            assistant_accent_color: palette::SOLARIZED_BLUE,
            reasoning_text_color: palette::SOLARIZED_ORANGE,
            reasoning_live_color: palette::SOLARIZED_ORANGE,
            status_success_color: palette::SOLARIZED_CYAN,
            status_warning_color: palette::SOLARIZED_ORANGE,
            status_error_color: palette::SOLARIZED_RED,
            status_info_color: palette::SOLARIZED_BLUE,
            diff_added_fg: palette::SOLARIZED_LIGHT_UI_THEME.diff_added_fg,
            diff_deleted_fg: palette::SOLARIZED_LIGHT_UI_THEME.diff_deleted_fg,
            diff_added_bg: palette::SOLARIZED_LIGHT_UI_THEME.diff_added_bg,
            diff_deleted_bg: palette::SOLARIZED_LIGHT_UI_THEME.diff_deleted_bg,
            tool_title_color: palette::SOLARIZED_TEXT_SOFT,
            tool_value_color: palette::SOLARIZED_TEXT_MUTED,
            tool_label_color: palette::SOLARIZED_TEXT_DIM,
            tool_running_accent: palette::SOLARIZED_BLUE,
            tool_success_accent: palette::SOLARIZED_CYAN,
            tool_failed_accent: palette::SOLARIZED_RED,
            plan_progress_color: palette::SOLARIZED_BLUE,
            plan_summary_color: palette::SOLARIZED_TEXT_MUTED,
            plan_explanation_color: palette::SOLARIZED_TEXT_DIM,
            plan_pending_color: palette::SOLARIZED_TEXT_MUTED,
            plan_in_progress_color: palette::SOLARIZED_ORANGE,
            plan_completed_color: palette::SOLARIZED_BLUE,
        }
    }

    /// Neutral black/white tokens for users who want minimal brand color.
    #[must_use]
    pub const fn grayscale() -> Self {
        Self {
            variant: Variant::Grayscale,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: palette::GRAYSCALE_BORDER,
            section_bg: palette::GRAYSCALE_PANEL,
            section_title_color: palette::GRAYSCALE_TEXT_SOFT,
            section_padding: Padding::horizontal(1),
            text_body_color: palette::GRAYSCALE_TEXT_BODY,
            text_muted_color: palette::GRAYSCALE_TEXT_MUTED,
            text_dim_color: palette::GRAYSCALE_TEXT_HINT,
            text_soft_color: palette::GRAYSCALE_TEXT_SOFT,
            user_accent_color: palette::GRAYSCALE_TEXT_BODY,
            user_message_bg: palette::GRAYSCALE_UI_THEME.elevated_bg,
            assistant_accent_color: palette::GRAYSCALE_TEXT_SOFT,
            reasoning_text_color: palette::GRAYSCALE_TEXT_MUTED,
            reasoning_live_color: palette::GRAYSCALE_TEXT_SOFT,
            status_success_color: palette::GRAYSCALE_TEXT_HINT,
            status_warning_color: palette::GRAYSCALE_TEXT_BODY,
            status_error_color: palette::GRAYSCALE_TEXT_BODY,
            status_info_color: palette::GRAYSCALE_TEXT_MUTED,
            diff_added_fg: palette::GRAYSCALE_UI_THEME.diff_added_fg,
            diff_deleted_fg: palette::GRAYSCALE_UI_THEME.diff_deleted_fg,
            diff_added_bg: palette::GRAYSCALE_UI_THEME.diff_added_bg,
            diff_deleted_bg: palette::GRAYSCALE_UI_THEME.diff_deleted_bg,
            tool_title_color: palette::GRAYSCALE_TEXT_SOFT,
            tool_value_color: palette::GRAYSCALE_TEXT_MUTED,
            tool_label_color: palette::GRAYSCALE_TEXT_HINT,
            tool_running_accent: palette::GRAYSCALE_TEXT_SOFT,
            tool_success_accent: palette::GRAYSCALE_TEXT_HINT,
            tool_failed_accent: palette::GRAYSCALE_TEXT_BODY,
            plan_progress_color: palette::GRAYSCALE_TEXT_SOFT,
            plan_summary_color: palette::GRAYSCALE_TEXT_MUTED,
            plan_explanation_color: palette::GRAYSCALE_TEXT_HINT,
            plan_pending_color: palette::GRAYSCALE_TEXT_MUTED,
            plan_in_progress_color: palette::GRAYSCALE_TEXT_BODY,
            plan_completed_color: palette::GRAYSCALE_TEXT_SOFT,
        }
    }

    #[must_use]
    pub const fn from_ui_theme(theme_id: ThemeId, ui: UiTheme) -> Self {
        let variant = match theme_id {
            ThemeId::DeepSeekShell => Variant::DeepSeekShell,
            _ => match ui.mode {
                PaletteMode::Dark => Variant::Dark,
                PaletteMode::Light | PaletteMode::SolarizedLight => Variant::Light,
                PaletteMode::Grayscale => Variant::Grayscale,
            },
        };
        Self {
            variant,
            section_borders: Borders::ALL,
            section_border_type: BorderType::Plain,
            section_border_color: ui.border,
            section_bg: ui.surface_bg,
            section_title_color: ui.accent_primary,
            section_padding: Padding::horizontal(1),
            text_body_color: ui.text_body,
            text_muted_color: ui.text_muted,
            text_dim_color: ui.text_dim,
            text_soft_color: ui.text_soft,
            user_accent_color: ui.accent_primary,
            user_message_bg: match theme_id {
                ThemeId::Whale => palette::USER_MESSAGE_BG,
                ThemeId::System => match ui.mode {
                    PaletteMode::Dark => palette::USER_MESSAGE_BG,
                    _ => ui.elevated_bg,
                },
                _ => ui.elevated_bg,
            },
            assistant_accent_color: ui.status_working,
            reasoning_text_color: ui.accent_secondary,
            reasoning_live_color: ui.accent_secondary,
            status_success_color: ui.success,
            status_warning_color: ui.status_warning,
            status_error_color: ui.error_fg,
            status_info_color: ui.info,
            diff_added_fg: ui.diff_added_fg,
            diff_deleted_fg: ui.diff_deleted_fg,
            diff_added_bg: ui.diff_added_bg,
            diff_deleted_bg: ui.diff_deleted_bg,
            tool_title_color: ui.text_soft,
            tool_value_color: ui.text_muted,
            tool_label_color: ui.text_hint,
            tool_running_accent: ui.tool_running,
            tool_success_accent: ui.tool_success,
            tool_failed_accent: ui.tool_failed,
            plan_progress_color: ui.status_working,
            plan_summary_color: ui.text_muted,
            plan_explanation_color: ui.text_hint,
            plan_pending_color: ui.text_muted,
            plan_in_progress_color: ui.warning,
            plan_completed_color: ui.success,
        }
    }

    #[must_use]
    pub const fn for_palette_mode(mode: PaletteMode) -> Self {
        match mode {
            PaletteMode::Dark => Self::dark(),
            PaletteMode::Light => Self::light(),
            PaletteMode::Grayscale => Self::grayscale(),
            PaletteMode::SolarizedLight => Self::solarized_light(),
        }
    }

    #[must_use]
    pub const fn for_theme_id(theme_id: ThemeId, mode: PaletteMode) -> Self {
        match theme_id {
            ThemeId::DeepSeekShell => Self::deepseek_shell(),
            _ => Self::for_palette_mode(mode),
        }
    }

    /// Pick the right tool accent for a given [`ToolStatus`].
    #[must_use]
    pub const fn tool_status_color(self, status: ToolStatus) -> Color {
        match status {
            ToolStatus::Running => self.tool_running_accent,
            ToolStatus::Success => self.tool_success_accent,
            ToolStatus::Failed => self.tool_failed_accent,
        }
    }

    /// Bold tool title style (e.g. "Plan", "Shell").
    #[must_use]
    pub fn tool_title_style(self) -> Style {
        Style::default()
            .fg(self.tool_title_color)
            .add_modifier(Modifier::BOLD)
    }

    /// Right-side status text ("running", "done", "issue") style.
    #[must_use]
    pub fn tool_status_style(self, status: ToolStatus) -> Style {
        Style::default().fg(self.tool_status_color(status))
    }

    /// Detail label style ("command:", "time:", step markers).
    #[must_use]
    pub fn tool_label_style(self) -> Style {
        Style::default().fg(self.tool_label_color)
    }

    /// Default value style for tool detail rows.
    #[must_use]
    pub fn tool_value_style(self) -> Style {
        Style::default().fg(self.tool_value_color)
    }
}

/// Returns the active theme used by the current render scope.
#[must_use]
pub fn active_theme() -> Theme {
    ACTIVE_THEME_OVERRIDE
        .with(|theme| theme.get())
        .unwrap_or_else(Theme::dark)
}

pub fn with_active_theme<T>(theme: Theme, f: impl FnOnce() -> T) -> T {
    ACTIVE_THEME_OVERRIDE.with(|slot| {
        struct ResetActiveTheme<'a> {
            slot: &'a Cell<Option<Theme>>,
            previous: Option<Theme>,
        }

        impl<'a> Drop for ResetActiveTheme<'a> {
            fn drop(&mut self) {
                self.slot.set(self.previous);
            }
        }

        let previous = slot.replace(Some(theme));
        let _reset = ResetActiveTheme { slot, previous };
        f()
    })
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{Theme, Variant, active_theme, with_active_theme};
    use crate::palette;
    use crate::tui::history::ToolStatus;

    #[test]
    fn active_theme_returns_dark() {
        assert_eq!(active_theme(), Theme::dark());
    }

    #[test]
    fn scoped_active_theme_restores_after_panic() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            with_active_theme(Theme::deepseek_shell(), || {
                assert_eq!(active_theme(), Theme::deepseek_shell());
                panic!("force scoped theme unwind");
            });
        }));

        assert!(result.is_err());
        assert_eq!(active_theme(), Theme::dark());
    }

    #[test]
    fn dark_theme_matches_existing_palette_choices() {
        let theme = Theme::dark();
        assert_eq!(theme.variant, Variant::Dark);
        assert_eq!(theme.section_border_color, palette::BORDER_COLOR);
        assert_eq!(theme.section_bg, palette::DEEPSEEK_INK);
        assert_eq!(theme.section_title_color, palette::DEEPSEEK_BLUE);
        assert_eq!(theme.tool_title_color, palette::TEXT_SOFT);
        assert_eq!(theme.tool_value_color, palette::TEXT_MUTED);
        assert_eq!(theme.tool_label_color, palette::TEXT_DIM);
        assert_eq!(theme.tool_running_accent, palette::ACCENT_TOOL_LIVE);
        assert_eq!(theme.tool_success_accent, palette::TEXT_DIM);
        assert_eq!(theme.tool_failed_accent, palette::ACCENT_TOOL_ISSUE);
    }

    #[test]
    fn deepseek_shell_theme_uses_shell_palette_tokens() {
        let theme = Theme::deepseek_shell();
        assert_eq!(theme.variant, Variant::DeepSeekShell);
        assert_eq!(
            theme.section_border_color,
            palette::DEEPSEEK_SHELL_UI_THEME.border
        );
        assert_eq!(
            theme.section_title_color,
            palette::DEEPSEEK_SHELL_UI_THEME.accent_primary
        );
        assert_eq!(
            theme.tool_running_accent,
            palette::DEEPSEEK_SHELL_UI_THEME.tool_running
        );
        assert_eq!(
            theme.plan_progress_color,
            palette::DEEPSEEK_SHELL_UI_THEME.status_working
        );
        assert_eq!(
            theme.diff_added_bg,
            palette::DEEPSEEK_SHELL_UI_THEME.diff_added_bg
        );
        assert_eq!(
            theme.diff_deleted_bg,
            palette::DEEPSEEK_SHELL_UI_THEME.diff_deleted_bg
        );
        assert_eq!(
            theme.user_message_bg_color(),
            palette::DEEPSEEK_SHELL_UI_THEME.elevated_bg
        );
    }

    #[test]
    fn for_theme_id_selects_deepseek_shell_tokens() {
        assert_eq!(
            Theme::for_theme_id(
                crate::palette::ThemeId::DeepSeekShell,
                crate::palette::PaletteMode::Dark,
            ),
            Theme::deepseek_shell()
        );
        assert_eq!(
            Theme::for_theme_id(
                crate::palette::ThemeId::WhaleLight,
                crate::palette::PaletteMode::Light,
            ),
            Theme::light()
        );
    }

    #[test]
    fn from_ui_theme_preserves_runtime_theme_tokens() {
        use ratatui::style::Color;

        let mut ui = palette::DEEPSEEK_SHELL_UI_THEME.with_background_color(Color::Indexed(17));
        ui.accent_primary = Color::Indexed(42);
        ui.diff_added_bg = Color::Indexed(22);
        ui.diff_deleted_bg = Color::Indexed(52);
        ui.elevated_bg = Color::Indexed(18);

        let theme = Theme::from_ui_theme(crate::palette::ThemeId::DeepSeekShell, ui);

        assert_eq!(theme.variant, Variant::DeepSeekShell);
        assert_eq!(theme.section_bg, Color::Indexed(17));
        assert_eq!(theme.section_title_color, Color::Indexed(42));
        assert_eq!(theme.diff_added_bg, Color::Indexed(22));
        assert_eq!(theme.diff_deleted_bg, Color::Indexed(52));
        assert_eq!(theme.user_message_bg_color(), Color::Indexed(18));
    }

    #[test]
    fn from_ui_theme_keeps_dark_default_user_message_background() {
        let theme = Theme::from_ui_theme(crate::palette::ThemeId::Whale, palette::UI_THEME);

        assert_eq!(theme.variant, Variant::Dark);
        assert_eq!(theme.user_message_bg_color(), palette::USER_MESSAGE_BG);
    }

    #[test]
    fn light_theme_uses_light_panel_tokens() {
        let theme = Theme::for_palette_mode(crate::palette::PaletteMode::Light);
        assert_eq!(theme.variant, Variant::Light);
        assert_eq!(theme.section_bg, palette::LIGHT_PANEL);
        assert_eq!(theme.section_border_color, palette::LIGHT_BORDER);
        assert_eq!(theme.tool_title_color, palette::LIGHT_TEXT_SOFT);
        assert_eq!(theme.tool_value_color, palette::LIGHT_TEXT_MUTED);
        assert_eq!(theme.plan_summary_color, palette::LIGHT_TEXT_MUTED);
    }

    #[test]
    fn grayscale_theme_uses_neutral_tokens() {
        let theme = Theme::for_palette_mode(crate::palette::PaletteMode::Grayscale);
        assert_eq!(theme.variant, Variant::Grayscale);
        assert_eq!(theme.section_bg, palette::GRAYSCALE_PANEL);
        assert_eq!(theme.section_border_color, palette::GRAYSCALE_BORDER);
        assert_eq!(theme.tool_running_accent, palette::GRAYSCALE_TEXT_SOFT);
        assert_eq!(theme.tool_failed_accent, palette::GRAYSCALE_TEXT_BODY);
        assert_eq!(theme.plan_summary_color, palette::GRAYSCALE_TEXT_MUTED);
    }

    #[test]
    fn tool_status_color_maps_each_status() {
        let theme = Theme::dark();
        assert_eq!(
            theme.tool_status_color(ToolStatus::Running),
            theme.tool_running_accent
        );
        assert_eq!(
            theme.tool_status_color(ToolStatus::Success),
            theme.tool_success_accent
        );
        assert_eq!(
            theme.tool_status_color(ToolStatus::Failed),
            theme.tool_failed_accent
        );
    }
}
