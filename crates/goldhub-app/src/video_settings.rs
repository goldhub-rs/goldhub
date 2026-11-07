//! GoldSrc-style Post-Processing & Color Grading controls (Gamma, Brightness, Contrast, Vibrance).
//!
//! Complies with the Scrooge Systems Mindset: zero runtime allocation,
//! utilizing Bevy's native GPU color grading pipeline pass and zero-overhead window title status.

use bevy::prelude::*;
use bevy::render::view::ColorGrading;

pub struct VideoSettingsPlugin;

impl Plugin for VideoSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VideoSettings>();
        app.add_systems(Startup, setup_video_window_title);
        app.add_systems(
            Update,
            (handle_video_settings_input, sync_color_grading_to_camera),
        );
    }
}

/// Live video settings mimicking GoldSrc `gamma` and `brightness` console variables.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct VideoSettings {
    /// Authentic GoldSrc gamma power curve (default: 2.5, range: 1.0..3.5).
    pub gamma: f32,
    /// Brightness multiplier (default: 1.0, range: 0.5..2.5).
    pub brightness: f32,
    /// Contrast multiplier (default: 1.0, range: 0.5..2.0).
    pub contrast: f32,
    /// Digital Vibrance / Saturation (default: 1.15, range: 0.0..3.0).
    pub vibrance: f32,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            gamma: 2.5,
            brightness: 1.0,
            contrast: 1.0,
            vibrance: 1.15,
        }
    }
}

impl VideoSettings {
    /// Formats the active video settings into a concise window title string.
    pub fn format_title(&self) -> String {
        format!(
            "GoldHub | Gamma: {:.2} ([/]) | Bright: {:.2} (-/=) | Contrast: {:.2} (;/') | Vibrance: {:.2} (,/.) | [Bksp: Reset]",
            self.gamma, self.brightness, self.contrast, self.vibrance
        )
    }

    /// Applies the current settings to Bevy's camera `ColorGrading` component.
    pub fn apply_to_color_grading(&self, grading: &mut ColorGrading) {
        grading.global.exposure = self.brightness - 1.0;
        grading.global.post_saturation = self.vibrance;

        grading.shadows.gamma = self.gamma;
        grading.midtones.gamma = self.gamma;
        grading.highlights.gamma = self.gamma;

        grading.shadows.contrast = self.contrast;
        grading.midtones.contrast = self.contrast;
        grading.highlights.contrast = self.contrast;
    }
}

fn setup_video_window_title(settings: Res<VideoSettings>, mut window_query: Query<&mut Window>) {
    let title = settings.format_title();
    for mut win in &mut window_query {
        win.title.clone_from(&title);
    }
}

#[allow(clippy::useless_let_if_seq)]
fn handle_video_settings_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<VideoSettings>,
    mut window_query: Query<&mut Window>,
) {
    let mut changed = false;

    // Reset to defaults: Backspace
    if keyboard.just_pressed(KeyCode::Backspace) {
        *settings = VideoSettings::default();
        changed = true;
    } else {
        // Gamma adjustments: [ / ]
        if keyboard.just_pressed(KeyCode::BracketLeft) {
            settings.gamma = (settings.gamma - 0.1).clamp(1.0, 3.5);
            changed = true;
        }
        if keyboard.just_pressed(KeyCode::BracketRight) {
            settings.gamma = (settings.gamma + 0.1).clamp(1.0, 3.5);
            changed = true;
        }

        // Brightness adjustments: Minus / Equal
        if keyboard.just_pressed(KeyCode::Minus) {
            settings.brightness = (settings.brightness - 0.05).clamp(0.5, 2.5);
            changed = true;
        }
        if keyboard.just_pressed(KeyCode::Equal) {
            settings.brightness = (settings.brightness + 0.05).clamp(0.5, 2.5);
            changed = true;
        }

        // Contrast adjustments: Semicolon / Quote
        if keyboard.just_pressed(KeyCode::Semicolon) {
            settings.contrast = (settings.contrast - 0.05).clamp(0.5, 2.0);
            changed = true;
        }
        if keyboard.just_pressed(KeyCode::Quote) {
            settings.contrast = (settings.contrast + 0.05).clamp(0.5, 2.0);
            changed = true;
        }

        // Digital Vibrance adjustments: Comma / Period
        if keyboard.just_pressed(KeyCode::Comma) {
            settings.vibrance = (settings.vibrance - 0.1).clamp(0.0, 3.0);
            changed = true;
        }
        if keyboard.just_pressed(KeyCode::Period) {
            settings.vibrance = (settings.vibrance + 0.1).clamp(0.0, 3.0);
            changed = true;
        }
    }

    if changed {
        let title = settings.format_title();
        for mut win in &mut window_query {
            win.title.clone_from(&title);
        }
        tracing::info!(
            gamma = settings.gamma,
            brightness = settings.brightness,
            contrast = settings.contrast,
            vibrance = settings.vibrance,
            "Video settings updated"
        );
    }
}

fn sync_color_grading_to_camera(
    settings: Res<VideoSettings>,
    mut camera_query: Query<&mut ColorGrading, With<Camera3d>>,
) {
    if settings.is_changed() {
        for mut grading in &mut camera_query {
            settings.apply_to_color_grading(&mut grading);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_settings_defaults_and_clamping() {
        let settings = VideoSettings::default();
        assert_eq!(settings.gamma, 2.5);
        assert_eq!(settings.brightness, 1.0);
        assert_eq!(settings.contrast, 1.0);
        assert_eq!(settings.vibrance, 1.15);

        let mut grading = ColorGrading::default();
        settings.apply_to_color_grading(&mut grading);

        assert_eq!(grading.shadows.gamma, 2.5);
        assert_eq!(grading.global.exposure, 0.0);
        assert_eq!(grading.global.post_saturation, 1.15);
    }
}
