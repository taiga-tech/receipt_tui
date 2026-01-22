//! 設定モデルと永続化ヘルパー。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// `config.toml` に保存するトップレベル設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Workerが利用するGoogle Drive/SheetsのID群。
    pub google: GoogleCfg,
    /// シート書き込み時に使うユーザー情報。
    pub user: UserCfg,
    /// テンプレートシート上のセル位置。
    pub template: TemplateCfg,
    /// 経費行の列レイアウト。
    pub general_expense: GeneralExpenseCfg,
}

/// Google API関連のID群。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCfg {
    /// 入力画像が置かれるDriveフォルダID。
    pub input_folder_id: String,
    /// PDFをアップロードするDriveフォルダID。
    pub output_folder_id: String,
    /// テンプレートスプレッドシートID（ショートカット可）。
    pub template_sheet_id: String,
}

/// テンプレートに挿入するユーザー情報。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCfg {
    /// テンプレートに記載する氏名。
    pub full_name: String,
}

/// テンプレートシート内のセル位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCfg {
    /// 氏名を入れるセル。
    pub name_cell: String,
    /// 対象月を入れるセル。
    pub target_month_cell: String,
}

/// 経費行のレイアウト情報。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralExpenseCfg {
    /// 経費入力の開始行。
    pub start_row: u32,
    /// 日付列。
    pub date_col: String,
    /// 用途（理由）列。
    pub reason_col: String,
    /// 金額列。
    pub amount_col: String,
    /// 勘定科目列。
    pub category_col: String,
    /// 備考列。
    pub note_col: String,
}

impl Config {
    /// ディスクから読み込み、無ければデフォルトを生成する。
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            // 既存ファイルを読み込んでTOMLとしてパースする。
            let s = fs::read_to_string(path)?;
            Ok(toml::from_str(&s)?)
        } else {
            // デフォルト設定を生成し、ファイルとして保存する。
            let cfg = Self::default();
            cfg.save(path)?;
            Ok(cfg)
        }
    }

    /// 設定を整形済みTOMLで保存する。
    pub fn save(&self, path: &Path) -> Result<()> {
        // TOML文字列に変換する。
        let s = toml::to_string_pretty(self)?;
        // 指定パスへ書き込む。
        fs::write(path, s)?;
        Ok(())
    }
}

impl Default for Config {
    /// Workerが期待するテンプレートのレイアウトに合わせた既定値。
    fn default() -> Self {
        Self {
            // Google API関連の既定値を設定する。
            google: GoogleCfg {
                input_folder_id: "".into(),
                output_folder_id: "".into(),
                template_sheet_id: "".into(),
            },
            // ユーザー情報の既定値を設定する。
            user: UserCfg {
                full_name: "Your Name".into(),
            },
            // テンプレート内のセル位置の既定値を設定する。
            template: TemplateCfg {
                name_cell: "F3".into(),
                target_month_cell: "B3".into(),
            },
            // 経費行のレイアウト既定値を設定する。
            general_expense: GeneralExpenseCfg {
                start_row: 7,
                date_col: "B".into(),
                reason_col: "C".into(),
                amount_col: "D".into(),
                category_col: "E".into(),
                note_col: "F".into(),
            },
        }
    }
}
