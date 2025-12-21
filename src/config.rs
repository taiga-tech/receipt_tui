//! Config model and persistence helpers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Top-level configuration stored in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Google Drive/Sheets ids used by the worker.
    pub google: GoogleCfg,
    /// User profile values used when writing the sheet.
    pub user: UserCfg,
    /// Template sheet cell positions.
    pub template: TemplateCfg,
    /// Column layout for the expense rows in the template.
    pub general_expense: GeneralExpenseCfg,
}

/// Google API related identifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCfg {
    /// Drive folder that holds input images.
    pub input_folder_id: String,
    /// Drive folder to upload exported PDFs.
    pub output_folder_id: String,
    /// Template spreadsheet id or shortcut id.
    pub template_sheet_id: String,
}

/// User metadata inserted into the template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCfg {
    /// Full name used in the template.
    pub full_name: String,
}

/// Cell addresses inside the template sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCfg {
    /// Cell containing the user's name.
    pub name_cell: String,
    /// Cell containing the target month.
    pub target_month_cell: String,
}

/// Layout information for the expense rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralExpenseCfg {
    /// First row where expense items start.
    pub start_row: u32,
    /// Column containing the date.
    pub date_col: String,
    /// Column containing the reason/description.
    pub reason_col: String,
    /// Column containing the amount.
    pub amount_col: String,
    /// Column containing the category.
    pub category_col: String,
    /// Column containing the note.
    pub note_col: String,
}

impl Config {
    /// Load from disk or create defaults when missing.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            let s = fs::read_to_string(path)?;
            Ok(toml::from_str(&s)?)
        } else {
            let cfg = Self::default();
            cfg.save(path)?;
            Ok(cfg)
        }
    }

    /// Persist the config as pretty TOML.
    pub fn save(&self, path: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        fs::write(path, s)?;
        Ok(())
    }
}

impl Default for Config {
    /// Defaults align with the template layout expected by the worker.
    fn default() -> Self {
        Self {
            google: GoogleCfg {
                input_folder_id: "".into(),
                output_folder_id: "".into(),
                template_sheet_id: "".into(),
            },
            user: UserCfg {
                full_name: "Your Name".into(),
            },
            template: TemplateCfg {
                name_cell: "F3".into(),
                target_month_cell: "B3".into(),
            },
            general_expense: GeneralExpenseCfg {
                start_row: 44,
                date_col: "B".into(),
                reason_col: "C".into(),
                amount_col: "D".into(),
                category_col: "E".into(),
                note_col: "F".into(),
            },
        }
    }
}
