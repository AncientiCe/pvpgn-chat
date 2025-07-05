use eframe::egui::{Color32, Style, FontId, FontFamily, TextStyle};

/// Application color scheme
#[derive(Debug, Clone)]
pub struct AppTheme {
    pub primary: Color32,
    pub secondary: Color32,
    pub accent: Color32,
    pub background: Color32,
    pub surface: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub success: Color32,
    #[allow(dead_code)]
    pub warning: Color32,
    pub error: Color32,
    pub typography: Typography,
}

impl AppTheme {
    /// Default light theme with proper contrast ratios
    pub fn light() -> Self {
        Self {
            primary: Color32::from_rgb(63, 81, 181),      // Indigo 500
            secondary: Color32::from_rgb(96, 125, 139),   // Blue Grey 500
            accent: Color32::from_rgb(255, 193, 7),       // Amber 500
            background: Color32::from_rgb(250, 250, 250), // Very light grey
            surface: Color32::WHITE,
            text_primary: Color32::from_rgb(33, 33, 33),  // Dark grey
            text_secondary: Color32::from_rgb(117, 117, 117), // Medium grey
            success: Color32::from_rgb(76, 175, 80),      // Green 500
            warning: Color32::from_rgb(255, 152, 0),      // Orange 500
            error: Color32::from_rgb(244, 67, 54),        // Red 500
            typography: Typography::new(),
        }
    }

    /// Dark theme with proper contrast ratios
    pub fn dark() -> Self {
        Self {
            primary: Color32::from_rgb(121, 134, 203),    // Indigo 300
            secondary: Color32::from_rgb(144, 164, 174),  // Blue Grey 300
            accent: Color32::from_rgb(255, 213, 79),      // Amber 300
            background: Color32::from_rgb(18, 18, 18),    // Very dark grey
            surface: Color32::from_rgb(33, 33, 33),       // Dark grey
            text_primary: Color32::from_rgb(255, 255, 255), // White
            text_secondary: Color32::from_rgb(189, 189, 189), // Light grey
            success: Color32::from_rgb(129, 199, 132),    // Green 300
            warning: Color32::from_rgb(255, 183, 77),     // Orange 300
            error: Color32::from_rgb(229, 115, 115),      // Red 300
            typography: Typography::new(),
        }
    }

    /// Apply theme to egui style
    pub fn apply_to_style(&self, style: &mut Style) {
        // Configure visuals
        let visuals = &mut style.visuals;

        // Background colors
        visuals.window_fill = self.surface;
        visuals.panel_fill = self.surface;
        visuals.faint_bg_color = self.background;

        // Text colors - use override_text_color for custom text colors
        visuals.override_text_color = Some(self.text_primary);

        // Widget colors with improved hover and focus states
        visuals.widgets.noninteractive.bg_fill = self.surface;
        visuals.widgets.noninteractive.weak_bg_fill = self.background;
        visuals.widgets.noninteractive.fg_stroke.color = self.text_primary;
        visuals.widgets.noninteractive.bg_stroke.width = 1.0;
        visuals.widgets.noninteractive.bg_stroke.color = self.text_secondary.gamma_multiply(0.3);

        // Improved input field styling for better readability
        visuals.widgets.inactive.bg_fill = self.surface;
        visuals.widgets.inactive.weak_bg_fill = self.background;
        visuals.widgets.inactive.fg_stroke.color = self.text_primary;
        visuals.widgets.inactive.bg_stroke.width = 1.0;
        visuals.widgets.inactive.bg_stroke.color = self.text_secondary.gamma_multiply(0.5);
        visuals.widgets.inactive.expansion = 0.0;

        visuals.widgets.hovered.bg_fill = self.primary.gamma_multiply(0.15);
        visuals.widgets.hovered.weak_bg_fill = self.primary.gamma_multiply(0.1);
        visuals.widgets.hovered.fg_stroke.color = self.text_primary;
        visuals.widgets.hovered.bg_stroke.width = 1.5;
        visuals.widgets.hovered.bg_stroke.color = self.primary.gamma_multiply(0.8);
        visuals.widgets.hovered.expansion = 1.0;

        visuals.widgets.active.bg_fill = self.primary;
        visuals.widgets.active.weak_bg_fill = self.primary.gamma_multiply(0.2);
        visuals.widgets.active.fg_stroke.color = Color32::WHITE;
        visuals.widgets.active.bg_stroke.width = 2.0;
        visuals.widgets.active.bg_stroke.color = self.primary;
        visuals.widgets.active.expansion = 0.0;

        // Button styling - rounding will be handled per-widget

        // Selection colors
        visuals.selection.bg_fill = self.primary.gamma_multiply(0.3);
        visuals.selection.stroke.color = self.primary;

        // Hyperlink colors
        visuals.hyperlink_color = self.accent;

        // Error colors
        visuals.error_fg_color = self.error;

        // Window and popup styling with enhanced shadows
        visuals.window_shadow.color = Color32::from_black_alpha(80);
        visuals.popup_shadow.color = Color32::from_black_alpha(60);

        // Enhanced window styling
        visuals.window_fill = self.surface;
        visuals.window_stroke.color = self.text_secondary.gamma_multiply(0.2);
        visuals.window_stroke.width = 1.0;

        // Panel styling with subtle borders
        visuals.panel_fill = self.surface;

        // Text edit specific styling for better readability
        visuals.extreme_bg_color = self.surface;
        visuals.code_bg_color = self.surface;

        // Apply typography settings
        self.typography.apply_to_style(style);
    }
}

/// Message type styling
#[derive(Debug, Clone)]
pub struct MessageStyle {
    pub system_color: Color32,
    pub user_color: Color32,
    pub whisper_color: Color32,
    pub join_leave_color: Color32,
    pub error_color: Color32,
}

impl MessageStyle {
    pub fn for_theme(theme: &AppTheme) -> Self {
        Self {
            system_color: theme.secondary,
            user_color: theme.text_primary,
            whisper_color: theme.accent,
            join_leave_color: theme.success,
            error_color: theme.error,
        }
    }
}

impl Default for AppTheme {
    fn default() -> Self {
        Self::light()
    }
}

impl Default for MessageStyle {
    fn default() -> Self {
        Self::for_theme(&AppTheme::default())
    }
}

/// Typography configuration for consistent text styling
#[derive(Debug, Clone)]
pub struct Typography {
    #[allow(dead_code)]
    pub heading_large: FontId,
    pub heading_medium: FontId,
    #[allow(dead_code)]
    pub heading_small: FontId,
    #[allow(dead_code)]
    pub body_large: FontId,
    pub body_medium: FontId,
    pub body_small: FontId,
    pub button: FontId,
    pub monospace: FontId,
}

impl Typography {
    pub fn new() -> Self {
        Self {
            heading_large: FontId::new(24.0, FontFamily::Proportional),
            heading_medium: FontId::new(20.0, FontFamily::Proportional),
            heading_small: FontId::new(16.0, FontFamily::Proportional),
            body_large: FontId::new(14.0, FontFamily::Proportional),
            body_medium: FontId::new(12.0, FontFamily::Proportional),
            body_small: FontId::new(10.0, FontFamily::Proportional),
            button: FontId::new(13.0, FontFamily::Proportional),
            monospace: FontId::new(12.0, FontFamily::Monospace),
        }
    }

    /// Apply typography settings to egui style
    pub fn apply_to_style(&self, style: &mut Style) {
        // Configure text styles
        style.text_styles.insert(TextStyle::Heading, self.heading_medium.clone());
        style.text_styles.insert(TextStyle::Body, self.body_medium.clone());
        style.text_styles.insert(TextStyle::Monospace, self.monospace.clone());
        style.text_styles.insert(TextStyle::Button, self.button.clone());
        style.text_styles.insert(TextStyle::Small, self.body_small.clone());

        // Add custom text styles for chat messages
        style.text_styles.insert(TextStyle::Name("chat_message".into()), self.body_medium.clone());
        style.text_styles.insert(TextStyle::Name("chat_username".into()), FontId::new(12.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Name("chat_system".into()), FontId::new(11.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Name("chat_whisper".into()), FontId::new(12.0, FontFamily::Proportional));

        // Configure spacing
        style.spacing.item_spacing = [8.0, 6.0].into();
        style.spacing.button_padding = [12.0, 6.0].into();
        style.spacing.menu_margin = 8.0.into();
        style.spacing.indent = 20.0;
        style.spacing.text_edit_width = 200.0;
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::new()
    }
}
