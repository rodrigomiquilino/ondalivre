use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JingleButton {
    pub name: String,
    pub path: Option<PathBuf>,
    pub loop_mode: bool,
    pub sweeper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JinglePage {
    pub name: String,
    pub buttons: Vec<JingleButton>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JingleBank {
    pub pages: Vec<JinglePage>,
}

impl Default for JingleBank {
    fn default() -> Self {
        let mut buttons = Vec::new();
        for i in 1..=12 {
            buttons.push(JingleButton {
                name: format!("Cart {i}"),
                path: None,
                loop_mode: false,
                sweeper: false,
            });
        }
        Self {
            pages: vec![JinglePage {
                name: "Página 1".into(),
                buttons,
            }],
        }
    }
}

impl JingleBank {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
