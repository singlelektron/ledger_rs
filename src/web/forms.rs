use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CreateAccountForm {
    pub(crate) name: String,
    pub(crate) currency: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RenameAccountForm {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTransactionForm {
    pub(crate) kind: String,
    pub(crate) amount: String,
    pub(crate) occurred_at: String,
    pub(crate) time_zone: String,
    pub(crate) description: String,
    pub(crate) category: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct TransactionQuery {
    pub(crate) kind: Option<String>,
    pub(crate) category: Option<String>,
    pub(crate) q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTransferForm {
    pub(crate) destination_account_id: u64,
    pub(crate) source_amount: String,
    pub(crate) destination_amount: String,
    pub(crate) occurred_at: String,
    pub(crate) time_zone: String,
    pub(crate) description: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateTransferForm {
    pub(crate) source_account_id: u64,
    pub(crate) destination_account_id: u64,
    pub(crate) source_amount: String,
    pub(crate) destination_amount: String,
    pub(crate) occurred_at: String,
    pub(crate) time_zone: String,
    pub(crate) description: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetBudgetForm {
    pub(crate) category: String,
    pub(crate) year: i32,
    pub(crate) month: u8,
    pub(crate) limit: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ReportQuery {
    pub(crate) account_id: Option<u64>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) time_zone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CsvImportForm {
    pub(crate) csv: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BackupRestoreForm {
    pub(crate) json: String,
}
