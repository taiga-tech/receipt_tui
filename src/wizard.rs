//! 初期設定ウィザードのステート管理。

/// ウィザードの各ステップ
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardStep {
    /// ウェルカムメッセージ
    Welcome,
    /// OAuth認証状態の確認
    CheckAuth,
    /// 入力フォルダID
    InputFolderId,
    /// 出力フォルダID
    OutputFolderId,
    /// テンプレートシートID
    TemplateSheetId,
    /// ユーザー名
    UserName,
    /// 完了
    Complete,
}

/// ウィザードの状態管理
#[derive(Clone, Debug)]
pub struct WizardState {
    /// 現在のステップ
    pub current_step: WizardStep,
    /// 全ステップ数
    pub total_steps: usize,
}

impl WizardState {
    /// 新しいウィザード状態を作成
    pub fn new() -> Self {
        // 最初はWelcomeステップから開始する。
        Self {
            current_step: WizardStep::Welcome,
            total_steps: 7,
        }
    }

    /// 次のステップへ進む
    pub fn next_step(&mut self) {
        // 現在のステップに応じて次のステップを決定する。
        self.current_step = match self.current_step {
            WizardStep::Welcome => WizardStep::CheckAuth,
            WizardStep::CheckAuth => WizardStep::InputFolderId,
            WizardStep::InputFolderId => WizardStep::OutputFolderId,
            WizardStep::OutputFolderId => WizardStep::TemplateSheetId,
            WizardStep::TemplateSheetId => WizardStep::UserName,
            WizardStep::UserName => WizardStep::Complete,
            WizardStep::Complete => WizardStep::Complete,
        };
    }

    /// 現在のステップのプロンプトメッセージを取得
    pub fn get_prompt(&self) -> String {
        // ステップごとの説明文を返す。
        match self.current_step {
            WizardStep::Welcome => {
                "receipt_tuiへようこそ！\n\nこのウィザードでは、アプリケーションの初期設定を行います。\nEnterキーを押して開始してください。".to_string()
            }
            WizardStep::CheckAuth => {
                "Google OAuth認証の確認中...\n\ncredentials.json が必要です。\nEnterキーで次へ進みます。".to_string()
            }
            WizardStep::InputFolderId => {
                "入力フォルダIDの設定\n\n領収書画像が保存されているGoogle DriveフォルダのIDを入力してください。\nEnterキーで入力画面を開きます。".to_string()
            }
            WizardStep::OutputFolderId => {
                "出力フォルダIDの設定\n\nPDFを保存するGoogle DriveフォルダのIDを入力してください。\nEnterキーで入力画面を開きます。".to_string()
            }
            WizardStep::TemplateSheetId => {
                "テンプレートシートIDの設定\n\n経費精算書テンプレートのGoogle Sheets IDを入力してください。\nEnterキーで入力画面を開きます。".to_string()
            }
            WizardStep::UserName => {
                "ユーザー名の設定\n\nあなたの氏名を入力してください。\nEnterキーで入力画面を開きます。".to_string()
            }
            WizardStep::Complete => {
                "設定完了！\n\nすべての設定が完了しました。\nEnterキーを押してメイン画面に移動します。".to_string()
            }
        }
    }

    /// 現在のステップ番号を取得（1始まり）
    pub fn get_step_number(&self) -> usize {
        // ステップを番号へ対応付ける。
        match self.current_step {
            WizardStep::Welcome => 1,
            WizardStep::CheckAuth => 2,
            WizardStep::InputFolderId => 3,
            WizardStep::OutputFolderId => 4,
            WizardStep::TemplateSheetId => 5,
            WizardStep::UserName => 6,
            WizardStep::Complete => 7,
        }
    }
}

impl Default for WizardState {
    fn default() -> Self {
        Self::new()
    }
}
