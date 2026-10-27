//! Sound bank (cartwall) buttons shown in the On air panel.

use serde::{Deserialize, Serialize};

pub const CART_PAGES: u8 = 8;
pub const CART_SLOTS: u8 = 24;

/// What a press does while the sound is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CartMode {
    /// Each press starts the sound from the beginning (again if it is playing).
    #[default]
    Fire,
    /// A press starts it; a press while it plays stops it.
    Toggle,
    /// Like Toggle, and it repeats until stopped.
    Loop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartButton {
    pub page: u8,
    pub slot: u8,
    pub track_id: Option<i64>,
    pub file_path: String,
    pub label: String,
    pub color: String,
    pub gain_db: f32,
    pub mode: CartMode,
    /// Lower the music while it sounds.
    pub duck: bool,
}

impl CartButton {
    /// Audio-side id of the button: page * CART_SLOTS + slot.
    pub fn id(&self) -> u32 {
        u32::from(self.page) * u32::from(CART_SLOTS) + u32::from(self.slot)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.page >= CART_PAGES {
            return Err(format!("Page must be 0 to {}", CART_PAGES - 1));
        }
        if self.slot >= CART_SLOTS {
            return Err(format!("Slot must be 0 to {}", CART_SLOTS - 1));
        }
        let label = self.label.trim();
        if label.is_empty() || label.chars().count() > 40 {
            return Err("Label must be 1 to 40 characters".to_string());
        }
        if !valid_color(&self.color) {
            return Err("Color must be empty or #rrggbb".to_string());
        }
        if !self.gain_db.is_finite() || !(-30.0..=12.0).contains(&self.gain_db) {
            return Err("Gain must be -30 to 12 dB".to_string());
        }
        Ok(())
    }
}

fn valid_color(color: &str) -> bool {
    if color.is_empty() {
        return true;
    }
    match color.strip_prefix('#') {
        Some(hex) => hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartBank {
    /// 0 for a bank that has not been saved yet.
    pub id: i64,
    pub name: String,
    pub buttons: Vec<CartButton>,
}

impl CartBank {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("Name must be 1 to 60 characters".to_string());
        }
        if self.buttons.len() > CART_PAGES as usize * CART_SLOTS as usize {
            return Err(format!(
                "A bank can hold at most {} buttons",
                CART_PAGES as usize * CART_SLOTS as usize
            ));
        }
        for button in &self.buttons {
            button.validate()?;
        }
        for (index, button) in self.buttons.iter().enumerate() {
            let duplicate = self.buttons[..index]
                .iter()
                .any(|other| other.page == button.page && other.slot == button.slot);
            if duplicate {
                return Err(format!(
                    "Two buttons cannot share page {} slot {}",
                    button.page, button.slot
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartBankSummary {
    pub id: i64,
    pub name: String,
    pub buttons: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(page: u8, slot: u8) -> CartButton {
        CartButton {
            page,
            slot,
            track_id: None,
            file_path: "/sounds/applause.wav".to_string(),
            label: "Applause".to_string(),
            color: String::new(),
            gain_db: 0.0,
            mode: CartMode::Fire,
            duck: false,
        }
    }

    #[test]
    fn button_validation_accepts_a_good_button() {
        let good = button(2, 5);
        assert!(good.validate().is_ok());
        assert_eq!(good.id(), 2 * 24 + 5);
    }

    #[test]
    fn button_validation_rejects_bad_values() {
        let mut bad = button(8, 0);
        assert!(bad.validate().is_err());

        bad = button(0, 24);
        assert!(bad.validate().is_err());

        bad = button(0, 0);
        bad.label = "   ".to_string();
        assert!(bad.validate().is_err());

        bad = button(0, 0);
        bad.color = "red".to_string();
        assert!(bad.validate().is_err());
        bad.color = "#12345g".to_string();
        assert!(bad.validate().is_err());
        bad.color = "#A1b2C3".to_string();
        assert!(bad.validate().is_ok());

        bad.color = String::new();
        bad.gain_db = 13.0;
        assert!(bad.validate().is_err());
        bad.gain_db = -30.0;
        assert!(bad.validate().is_ok());
        bad.gain_db = 12.0;
        assert!(bad.validate().is_ok());
        bad.gain_db = -30.1;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn bank_validation_rejects_duplicate_slots_and_bad_names() {
        let mut bank = CartBank {
            id: 0,
            name: "Breakfast".to_string(),
            buttons: vec![button(0, 0), button(0, 1)],
        };
        assert!(bank.validate().is_ok());

        bank.buttons[1].slot = 0;
        assert!(bank.validate().is_err());

        bank.buttons.pop();
        bank.name = " ".to_string();
        assert!(bank.validate().is_err());

        bank.name = "x".repeat(61);
        assert!(bank.validate().is_err());

        bank.name = "x".repeat(60);
        assert!(bank.validate().is_ok());
    }
}
