//! The bell: the flash of the visual bell and the bell command.

use std::path::Path;
use std::time::Duration;

use iced::Color;
use log::warn;

use crate::config::bell::{BellAnimation, BellConfig, Spring};
use crate::config::ui_config::Program;
use crate::daemon::spawn_daemon;

/// What happens when a terminal rings the bell.
#[derive(Debug, Clone, PartialEq)]
pub struct Bell {
    /// Program run with every bell.
    pub command: Option<Program>,
    pub flash: Flash,
}

impl Bell {
    pub fn from_config(config: &BellConfig) -> Self {
        Self { command: config.command.clone(), flash: Flash::from_config(config) }
    }

    /// Run the bell command, if there is one, in `working_directory` if that is known.
    pub fn run_command(&self, working_directory: Option<&Path>) {
        let Some(command) = &self.command else { return };

        if let Err(err) = spawn_daemon(command.program(), command.args(), working_directory) {
            warn!("Unable to run the bell command {:?}: {err}", command.program());
        }
    }
}

/// The visual bell: a color drawn over the terminal that rang the bell, fading out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flash {
    animation: BellAnimation,
    spring: Spring,
    /// How long the flash takes to fade out; none at all turns it off.
    duration: Duration,
    color: Color,
}

impl Flash {
    pub fn from_config(config: &BellConfig) -> Self {
        Self {
            animation: config.animation,
            spring: config.spring,
            duration: config.duration(),
            color: Color::from_rgb8(config.color.r, config.color.g, config.color.b),
        }
    }

    /// Whether the bell flashes at all.
    pub fn is_enabled(&self) -> bool {
        !self.duration.is_zero()
    }

    /// Whether the flash of a bell that rang `elapsed` ago is over.
    pub fn is_over(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }

    /// How strong the flash is `elapsed` after the bell rang: 1 right away, 0 once it is over.
    pub fn intensity(&self, elapsed: Duration) -> f64 {
        if self.is_over(elapsed) {
            return 0.;
        }

        // How far the flash has faded, from 0 to 1, for the share of its duration that passed.
        let time = elapsed.as_secs_f64() / self.duration.as_secs_f64();
        let faded = match self.animation {
            BellAnimation::Ease | BellAnimation::EaseOut => cubic_bezier(0.25, 0.1, 0.25, 1., time),
            BellAnimation::EaseOutSine => cubic_bezier(0.39, 0.575, 0.565, 1., time),
            BellAnimation::EaseOutQuad => cubic_bezier(0.25, 0.46, 0.45, 0.94, time),
            BellAnimation::EaseOutCubic => cubic_bezier(0.215, 0.61, 0.355, 1., time),
            BellAnimation::EaseOutQuart => cubic_bezier(0.165, 0.84, 0.44, 1., time),
            BellAnimation::EaseOutQuint => cubic_bezier(0.23, 1., 0.32, 1., time),
            BellAnimation::EaseOutExpo => cubic_bezier(0.19, 1., 0.22, 1., time),
            BellAnimation::EaseOutCirc => cubic_bezier(0.075, 0.82, 0.165, 1., time),
            BellAnimation::Linear => time,
            // A spring that overshoots its target shows as a weaker flash, so the bell bounces.
            BellAnimation::Spring => 1. - (1. - self.spring.position(elapsed.as_secs_f64())).abs(),
        };

        (1. - faded).clamp(0., 1.)
    }

    /// The color to draw over a terminal `elapsed` after it rang the bell, while it flashes.
    pub fn color(&self, elapsed: Duration) -> Option<Color> {
        let intensity = self.intensity(elapsed) as f32;
        (intensity > 0.).then_some(Color { a: intensity, ..self.color })
    }
}

fn cubic_bezier(p0: f64, p1: f64, p2: f64, p3: f64, x: f64) -> f64 {
    (1. - x).powi(3) * p0
        + 3. * (1. - x).powi(2) * x * p1
        + 3. * (1. - x) * x.powi(2) * p2
        + x.powi(3) * p3
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    fn flash(config: &str) -> Flash {
        let config: UiConfig = toml::from_str(config).unwrap();
        Bell::from_config(&config.bell).flash
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn visual_bell_is_off_by_default_and_without_a_duration() {
        let off = flash("");
        assert!(!off.is_enabled());
        assert_eq!(off.intensity(Duration::ZERO), 0.);
        assert_eq!(off.color(Duration::ZERO), None);

        assert!(!flash("[bell]\nduration = 0\nanimation = \"Spring\"\n").is_enabled());
        assert!(flash("[bell]\nduration = 1\n").is_enabled());
    }

    #[test]
    fn linear_flash_fades_evenly_over_its_duration() {
        let flash = flash("[bell]\nduration = 200\n");

        assert_eq!(flash.intensity(Duration::ZERO), 1.);
        assert!((flash.intensity(ms(50)) - 0.75).abs() < 1e-9);
        assert!((flash.intensity(ms(100)) - 0.5).abs() < 1e-9);
        assert!(!flash.is_over(ms(199)));
        assert!(flash.is_over(ms(200)));
        assert_eq!(flash.intensity(ms(200)), 0.);
        assert_eq!(flash.intensity(ms(5000)), 0.);
    }

    #[test]
    fn flash_has_the_bell_color_at_its_intensity() {
        let flash = flash("[bell]\nduration = 200\ncolor = \"#ff8000\"\n");

        assert_eq!(flash.color(Duration::ZERO), Some(Color::from_rgb8(0xff, 0x80, 0x00)));
        let half = flash.color(ms(100)).unwrap();
        assert_eq!((half.r, half.g, half.b), (1., 128. / 255., 0.));
        assert!((half.a - 0.5).abs() < 1e-6);
        assert_eq!(flash.color(ms(200)), None);
    }

    #[test]
    fn easings_start_bright_and_fade_out() {
        let animations = [
            "Ease",
            "EaseOut",
            "EaseOutSine",
            "EaseOutQuad",
            "EaseOutCubic",
            "EaseOutQuart",
            "EaseOutQuint",
            "EaseOutExpo",
            "EaseOutCirc",
            "Linear",
        ];

        for animation in animations {
            let flash = flash(&format!("[bell]\nduration = 300\nanimation = \"{animation}\"\n"));
            let intensities: Vec<_> = (0..=300).map(|step| flash.intensity(ms(step))).collect();

            // The curves are Alacritty's, which don't all start at the full color.
            assert!(intensities[0] > 0.6, "{animation} starts at {}", intensities[0]);
            assert!(
                intensities.iter().all(|intensity| (0. ..=1.).contains(intensity)),
                "{animation} leaves the range"
            );
            assert!(intensities[299] < 0.1, "{animation} ends at {}", intensities[299]);
            assert_eq!(intensities[300], 0., "{animation}");
        }

        // The easings have faded further than the linear flash early on.
        let linear = flash("[bell]\nduration = 300\n").intensity(ms(100));
        let eased = flash("[bell]\nduration = 300\nanimation = \"EaseOutExpo\"\n");
        assert!(eased.intensity(ms(100)) < linear);
    }

    #[test]
    fn spring_flash_stays_in_range_and_ends() {
        let flash = flash("[bell]\nanimation = 'Spring'\nduration = 400\n");

        assert_eq!(flash.intensity(Duration::ZERO), 1.);
        for step in 0..=50 {
            let intensity = flash.intensity(ms(step * 8));
            assert!((0. ..=1.).contains(&intensity), "{intensity} at step {step}");
        }
        assert_eq!(flash.intensity(ms(400)), 0.);
    }

    #[test]
    fn underdamped_spring_flash_bounces() {
        let flash = flash(
            "[bell]\nanimation = 'Spring'\nduration = 1000\nspring = { frequency = 18.0, damping \
             = 0.2 }\n",
        );
        let intensities: Vec<_> = (0..=100).map(|step| flash.intensity(ms(step * 10))).collect();

        // After first fading out, the overshoot flashes again.
        let first_low = intensities.iter().position(|&i| i < 0.05).unwrap();
        assert!(intensities[first_low..].iter().any(|&i| i > 0.2));
    }

    #[test]
    fn accessibility_turns_the_flash_off_and_keeps_the_command() {
        let mut config: UiConfig =
            toml::from_str("[bell]\nduration = 200\ncommand = \"notify-send\"\n").unwrap();
        config.apply_accessibility(true);

        let bell = Bell::from_config(&config.bell);
        assert!(!bell.flash.is_enabled());
        assert_eq!(bell.command.as_ref().map(Program::program), Some("notify-send"));
    }
}
