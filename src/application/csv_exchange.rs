use crate::application::list_transactions::{
    ListTransactionsError, TransactionFilter, list_account_transactions,
};
use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{Category, NewTransaction, Transaction, TransactionKind};
use jiff::Zoned;

const HEADER: [&str; 7] = [
    "account_id",
    "kind",
    "amount_minor",
    "currency",
    "occurred_at",
    "description",
    "category",
];

const USER_HEADER: [&str; 7] = [
    "account",
    "kind",
    "amount",
    "currency",
    "occurred_at",
    "description",
    "category",
];

// Parse positive decimal currency amounts exactly, without floating-point rounding.
fn parse_decimal_minor(value: &str) -> Option<i64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?.checked_mul(100)?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()? * 10,
        _ => fraction.parse::<i64>().ok()?,
    };
    whole.checked_add(fraction)
}

#[derive(Debug, PartialEq, Eq)]
pub enum CsvExchangeError {
    InvalidCsv { line: usize, message: String },
    InvalidHeader,
    InvalidRow { line: usize, message: String },
    List(ListTransactionsError),
    Repository(RepositoryError),
}

impl From<ListTransactionsError> for CsvExchangeError {
    fn from(error: ListTransactionsError) -> Self {
        Self::List(error)
    }
}

impl From<RepositoryError> for CsvExchangeError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

fn currency_code(value: Currency) -> &'static str {
    match value {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

fn parse_currency(value: &str) -> Option<Currency> {
    match value.to_ascii_uppercase().as_str() {
        "CNY" => Some(Currency::Cny),
        "USD" => Some(Currency::Usd),
        "EUR" => Some(Currency::Eur),
        "HKD" => Some(Currency::Hkd),
        "MYR" => Some(Currency::Myr),
        _ => None,
    }
}

fn kind_code(value: TransactionKind) -> &'static str {
    match value {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::ExpenseRefund => "expense_refund",
    }
}

fn parse_kind(value: &str) -> Option<TransactionKind> {
    match value.to_ascii_lowercase().as_str() {
        "income" => Some(TransactionKind::Income),
        "expense" => Some(TransactionKind::Expense),
        "expense_refund" => Some(TransactionKind::ExpenseRefund),
        _ => None,
    }
}

fn category_code(value: Category) -> &'static str {
    match value {
        Category::Food => "food",
        Category::Transportation => "transportation",
        Category::Entertainment => "entertainment",
        Category::Necessary => "necessary",
        Category::Health => "health",
        Category::Education => "education",
        Category::Shopping => "shopping",
        Category::Travel => "travel",
        Category::Housing => "housing",
        Category::Salary => "salary",
        Category::Sale => "sale",
        Category::Family => "family",
        Category::Investment => "investment",
        Category::Other => "other",
    }
}

fn parse_category(value: &str) -> Option<Category> {
    match value.to_ascii_lowercase().as_str() {
        "food" => Some(Category::Food),
        "transportation" => Some(Category::Transportation),
        "entertainment" => Some(Category::Entertainment),
        "necessary" => Some(Category::Necessary),
        "health" => Some(Category::Health),
        "education" => Some(Category::Education),
        "shopping" => Some(Category::Shopping),
        "travel" => Some(Category::Travel),
        "housing" => Some(Category::Housing),
        "salary" => Some(Category::Salary),
        "sale" => Some(Category::Sale),
        "family" => Some(Category::Family),
        "investment" => Some(Category::Investment),
        "other" => Some(Category::Other),
        _ => None,
    }
}

fn escape_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn parse_csv(input: &str) -> Result<Vec<(usize, Vec<String>)>, CsvExchangeError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;
    let mut line = 1usize;
    let mut row_line = 1usize;
    while let Some(character) = chars.next() {
        if in_quotes {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    field.push('\n');
                    line += 1;
                }
                other => field.push(other),
            }
        } else {
            match character {
                '"' if field.is_empty() => in_quotes = true,
                '"' => {
                    return Err(CsvExchangeError::InvalidCsv {
                        line,
                        message: "quote inside unquoted field".to_string(),
                    });
                }
                ',' => row.push(std::mem::take(&mut field)),
                '\r' if chars.peek() == Some(&'\n') => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push((row_line, std::mem::take(&mut row)));
                    line += 1;
                    row_line = line;
                }
                other => field.push(other),
            }
        }
    }
    if in_quotes {
        return Err(CsvExchangeError::InvalidCsv {
            line,
            message: "unterminated quoted field".to_string(),
        });
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push((row_line, row));
    }
    Ok(rows)
}

pub fn export_transactions_csv(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    account_id: AccountId,
    filter: TransactionFilter,
) -> Result<String, CsvExchangeError> {
    let transactions = list_account_transactions(accounts, transactions, account_id, filter)?;
    let mut output = format!("{}\n", HEADER.join(","));
    for transaction in transactions {
        let fields = [
            transaction.account_id().value().to_string(),
            kind_code(transaction.kind()).to_string(),
            transaction.amount().minor_units().to_string(),
            currency_code(transaction.amount().currency()).to_string(),
            transaction.occurred_at().to_string(),
            transaction.description().to_string(),
            category_code(transaction.category()).to_string(),
        ];
        output.push_str(&fields.map(|field| escape_field(&field)).join(","));
        output.push('\n');
    }
    Ok(output)
}

pub fn import_transactions_csv(
    accounts: &impl AccountRepository,
    transactions: &mut impl TransactionRepository,
    input: &str,
) -> Result<Vec<Transaction>, CsvExchangeError> {
    let mut rows = parse_csv(input)?.into_iter();
    let Some((_, header)) = rows.next() else {
        return Err(CsvExchangeError::InvalidHeader);
    };
    let user_format = header == USER_HEADER;
    if header != HEADER && !user_format {
        return Err(CsvExchangeError::InvalidHeader);
    }
    let named_accounts = if user_format {
        accounts.find_all()?
    } else {
        Vec::new()
    };
    let mut parsed = Vec::new();
    for (line, row) in rows {
        if row.len() != HEADER.len() {
            return Err(CsvExchangeError::InvalidRow {
                line,
                message: format!("expected {} columns, found {}", HEADER.len(), row.len()),
            });
        }
        let invalid = |message: &str| CsvExchangeError::InvalidRow {
            line,
            message: message.to_string(),
        };
        let account = if user_format {
            let mut matches = named_accounts
                .iter()
                .filter(|account| account.name() == row[0]);
            let account = matches
                .next()
                .ok_or_else(|| invalid("unknown account name"))?;
            if matches.next().is_some() {
                return Err(invalid("ambiguous account name"));
            }
            account.clone()
        } else {
            let id = AccountId::new(
                row[0]
                    .parse::<u64>()
                    .map_err(|_| invalid("invalid account_id"))?,
            );
            accounts
                .find_by_id(id)?
                .ok_or_else(|| invalid("account not found"))?
        };
        let account_id = account.id();
        let kind = parse_kind(&row[1]).ok_or_else(|| invalid("invalid kind"))?;
        let amount_minor = if user_format {
            parse_decimal_minor(&row[2]).ok_or_else(|| invalid("invalid amount: expected a positive decimal with at most two fractional digits"))?
        } else {
            row[2]
                .parse::<i64>()
                .map_err(|_| invalid("invalid amount_minor"))?
        };
        let currency = parse_currency(&row[3]).ok_or_else(|| invalid("invalid currency"))?;
        if currency != account.currency() {
            return Err(invalid("currency does not match account"));
        }
        let occurred_at = row[4]
            .parse::<Zoned>()
            .map_err(|_| invalid("invalid occurred_at"))?;
        let category = parse_category(&row[6]).ok_or_else(|| invalid("invalid category"))?;
        parsed.push(
            NewTransaction::new(
                account_id,
                kind,
                Money::from_minor_units(amount_minor, currency),
                occurred_at,
                row[5].clone(),
                category,
            )
            .map_err(|error| invalid(&format!("invalid transaction: {error:?}")))?,
        );
    }
    Ok(transactions.create_many(parsed)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{AccountRepository, TransactionRepository};
    use crate::domain::account::NewAccount;
    use crate::domain::money::Currency;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    #[test]
    fn user_format_imports_exact_amounts_and_native_export_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let (mut accounts, mut transactions) =
            crate::infrastructure::sqlite::open_repositories(directory.path().join("ledger.db"))
                .unwrap();
        let account = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        let input = format!(
            "{}\nCash,expense,12.50,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Lunch,food\nCash,income,7.8,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Pay,salary\n",
            USER_HEADER.join(",")
        );
        let created = import_transactions_csv(&accounts, &mut transactions, &input).unwrap();
        assert_eq!(created[0].amount().minor_units(), 1250);
        assert_eq!(created[1].amount().minor_units(), 780);
        let exported = export_transactions_csv(
            &accounts,
            &transactions,
            account.id(),
            TransactionFilter::default(),
        )
        .unwrap();
        assert!(exported.starts_with(&HEADER.join(",")));
        assert_eq!(
            import_transactions_csv(&accounts, &mut transactions, &exported)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn invalid_user_rows_leave_sqlite_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let (mut accounts, mut transactions) =
            crate::infrastructure::sqlite::open_repositories(directory.path().join("ledger.db"))
                .unwrap();
        let account = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        for (name, amount, currency, expected) in [
            ("Missing", "1", "CNY", "unknown account name"),
            ("Cash", "1.001", "CNY", "invalid amount"),
            ("Cash", "-1", "CNY", "invalid amount"),
            ("Cash", "0", "CNY", "invalid transaction"),
            ("Cash", "NaN", "CNY", "invalid amount"),
            ("Cash", "92233720368547758.08", "CNY", "invalid amount"),
            ("Cash", "1", "USD", "currency does not match account"),
        ] {
            let input = format!(
                "{}\nCash,expense,1,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Good,food\n{name},expense,{amount},{currency},2026-08-20T10:00:00+08:00[Asia/Shanghai],Bad,food\n",
                USER_HEADER.join(",")
            );
            match import_transactions_csv(&accounts, &mut transactions, &input).unwrap_err() {
                CsvExchangeError::InvalidRow { line, message } => {
                    assert_eq!(line, 3);
                    assert!(message.starts_with(expected), "{message}");
                }
                error => panic!("unexpected error: {error:?}"),
            }
            assert!(
                transactions
                    .find_by_account_id(account.id())
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn duplicate_names_are_ambiguous_even_with_different_currencies() {
        let mut accounts = InMemoryAccountRepository::new();
        for currency in [Currency::Cny, Currency::Usd] {
            accounts
                .create(NewAccount::new("Cash".into(), currency).unwrap())
                .unwrap();
        }
        let mut transactions = InMemoryTransactionRepository::new();
        let input = format!(
            "{}\nCash,expense,1,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Lunch,food\n",
            USER_HEADER.join(",")
        );
        assert_eq!(
            import_transactions_csv(&accounts, &mut transactions, &input),
            Err(CsvExchangeError::InvalidRow {
                line: 2,
                message: "ambiguous account name".into()
            })
        );
    }

    #[test]
    fn decimal_parser_checks_precision_and_overflow() {
        assert_eq!(parse_decimal_minor("92233720368547758.07"), Some(i64::MAX));
        for value in [
            "",
            ".1",
            "1.",
            "1.234",
            "1e2",
            " 1",
            "+1",
            "1,000",
            "92233720368547758.08",
        ] {
            assert_eq!(parse_decimal_minor(value), None, "{value}");
        }
    }

    #[test]
    fn round_trips_quoted_unicode_csv() {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        let input = concat!(
            "account_id,kind,amount_minor,currency,occurred_at,description,category\n",
            "1,expense,1250,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],\"晚餐, \"\"朋友\"\"\",food\n"
        );
        let created = import_transactions_csv(&accounts, &mut transactions, input).unwrap();
        assert_eq!(created[0].description(), "晚餐, \"朋友\"");
        let output = export_transactions_csv(
            &accounts,
            &transactions,
            account.id(),
            TransactionFilter::default(),
        )
        .unwrap();
        assert!(output.contains("\"晚餐, \"\"朋友\"\"\""));
    }

    #[test]
    fn invalid_later_row_writes_nothing() {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        let input = concat!(
            "account_id,kind,amount_minor,currency,occurred_at,description,category\n",
            "1,expense,100,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Lunch,food\n",
            "1,expense,broken,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Dinner,food\n"
        );
        assert_eq!(
            import_transactions_csv(&accounts, &mut transactions, input),
            Err(CsvExchangeError::InvalidRow {
                line: 3,
                message: "invalid amount_minor".to_string(),
            })
        );
        assert!(
            transactions
                .find_by_account_id(account.id())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_an_empty_file_with_a_specific_error() {
        let accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();

        assert_eq!(
            import_transactions_csv(&accounts, &mut transactions, ""),
            Err(CsvExchangeError::InvalidHeader)
        );
    }
}
