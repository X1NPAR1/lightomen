// i18n.rs: Localization support (Turkish & English) for LightOmen Linux
// Implements clean language selection while preserving standard hardware terms.

use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Tr,
    En,
}

#[allow(dead_code)]
impl Language {
    pub fn load() -> Self {
        if let Ok(user_home) = std::env::var("HOME") {
            let p = PathBuf::from(user_home).join(".config/lightomen/language");
            if let Ok(s) = fs::read_to_string(&p) {
                let trimmed = s.trim().to_lowercase();
                if trimmed == "tr" {
                    return Language::Tr;
                } else if trimmed == "en" {
                    return Language::En;
                }
            }
        }
        if std::env::var("LANG").unwrap_or_default().starts_with("tr") {
            Language::Tr
        } else {
            Language::En
        }
    }

    pub fn save(&self) {
        if let Ok(user_home) = std::env::var("HOME") {
            let dir = PathBuf::from(user_home).join(".config/lightomen");
            let _ = fs::create_dir_all(&dir);
            let p = dir.join("language");
            let code = match self {
                Language::Tr => "tr\n",
                Language::En => "en\n",
            };
            let _ = fs::write(p, code);
        }
    }

    #[inline]
    pub fn is_tr(&self) -> bool {
        *self == Language::Tr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_is_tr() {
        assert!(Language::Tr.is_tr());
        assert!(!Language::En.is_tr());
    }
}
