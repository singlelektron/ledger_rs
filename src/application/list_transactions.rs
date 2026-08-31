use jiff::Zoned;

use crate::{
    application::repository::{AccountRepository, RepositoryError, TransactionRepository},
    domain::{
        account::AccountId,
        transaction::{Category, Transaction, TransactionKind},
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum ListTransactionsError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
    TimeRangeError { from: Zoned, to: Zoned },
    InvalidDescriptionFilter,
    InvalidAmountRange { min: Option<i64>, max: Option<i64> },
    InvalidPageLimit { limit: usize, max: usize },
}

impl From<RepositoryError> for ListTransactionsError {
    fn from(error: RepositoryError) -> Self {
        ListTransactionsError::Repository(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransactionFilter {
    pub category: Option<Category>,
    pub kind: Option<TransactionKind>,
    pub from: Option<Zoned>,
    pub to: Option<Zoned>,
    pub description_contains: Option<String>,
    pub min_amount_minor: Option<i64>,
    pub max_amount_minor: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionCursor {
    pub occurred_at: Zoned,
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionPageRequest {
    pub limit: usize,
    pub cursor: Option<TransactionCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionPage {
    pub items: Vec<Transaction>,
    pub next_cursor: Option<TransactionCursor>,
}

pub const MAX_TRANSACTION_PAGE_SIZE: usize = 200;

pub fn list_account_transactions(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
    filter: TransactionFilter,
) -> Result<Vec<Transaction>, ListTransactionsError> {
    let account = account_repository.find_by_id(account_id)?;
    let account = account.ok_or(ListTransactionsError::AccountNotFound(account_id))?;

    let mut transactions = transaction_repository.find_by_account_id(account.id())?;

    if let Some(category) = filter.category {
        transactions.retain(|transaction| transaction.category() == category);
    }
    if let Some(kind) = filter.kind {
        transactions.retain(|transaction| transaction.kind() == kind);
    }

    if let Some(description) = filter.description_contains {
        let description = description.trim().to_lowercase();
        if description.is_empty() {
            return Err(ListTransactionsError::InvalidDescriptionFilter);
        }
        transactions.retain(|transaction| {
            transaction
                .description()
                .to_lowercase()
                .contains(&description)
        });
    }

    if filter.min_amount_minor.is_some_and(|value| value <= 0)
        || filter.max_amount_minor.is_some_and(|value| value <= 0)
        || matches!(
            (filter.min_amount_minor, filter.max_amount_minor),
            (Some(min), Some(max)) if min > max
        )
    {
        return Err(ListTransactionsError::InvalidAmountRange {
            min: filter.min_amount_minor,
            max: filter.max_amount_minor,
        });
    }
    if let Some(min) = filter.min_amount_minor {
        transactions.retain(|transaction| transaction.amount().minor_units() >= min);
    }
    if let Some(max) = filter.max_amount_minor {
        transactions.retain(|transaction| transaction.amount().minor_units() <= max);
    }

    if let (Some(from), Some(to)) = (&filter.from, &filter.to)
        && from >= to
    {
        return Err(ListTransactionsError::TimeRangeError {
            from: from.clone(),
            to: to.clone(),
        });
    }

    if let Some(from) = filter.from {
        transactions.retain(|transaction| transaction.occurred_at() >= from);
    }

    if let Some(to) = filter.to {
        transactions.retain(|transaction| transaction.occurred_at() < to);
    }

    transactions.sort_by(|a, b| {
        b.occurred_at()
            .cmp(a.occurred_at())
            .then_with(|| b.id().value().cmp(&a.id().value()))
    });

    Ok(transactions)
}

pub fn list_account_transaction_page(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
    filter: TransactionFilter,
    request: TransactionPageRequest,
) -> Result<TransactionPage, ListTransactionsError> {
    if request.limit == 0 || request.limit > MAX_TRANSACTION_PAGE_SIZE {
        return Err(ListTransactionsError::InvalidPageLimit {
            limit: request.limit,
            max: MAX_TRANSACTION_PAGE_SIZE,
        });
    }
    let mut items = list_account_transactions(
        account_repository,
        transaction_repository,
        account_id,
        filter,
    )?;
    if let Some(cursor) = request.cursor {
        items.retain(|transaction| {
            transaction.occurred_at() < cursor.occurred_at
                || (transaction.occurred_at() == cursor.occurred_at
                    && transaction.id().value() < cursor.id)
        });
    }
    let has_more = items.len() > request.limit;
    items.truncate(request.limit);
    let next_cursor = if has_more {
        items.last().map(|last| TransactionCursor {
            occurred_at: last.occurred_at().clone(),
            id: last.id().value(),
        })
    } else {
        None
    };
    Ok(TransactionPage { items, next_cursor })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::Account;
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, TransactionId, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };
    use crate::infrastructure::sqlite::in_memory_repositories;

    fn build_sample_transactions(
        transaction_repository: &mut impl TransactionRepository,
        account_id: AccountId,
    ) -> &impl TransactionRepository {
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Lunch"),
            crate::domain::transaction::Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(2_000, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
            Category::Salary,
        )
        .unwrap();
        let transaction3 = Transaction::new(
            TransactionId::new(3),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(500, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Dinner"),
            Category::Food,
        )
        .unwrap();
        let transaction4 = Transaction::new(
            TransactionId::new(4),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(500, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Dinner"),
            Category::Food,
        )
        .unwrap();
        transaction_repository.save(transaction3).unwrap();
        transaction_repository.save(transaction4).unwrap();
        transaction_repository.save(transaction1).unwrap();
        transaction_repository.save(transaction2).unwrap();
        transaction_repository
    }

    #[test]
    fn lists_transactions_for_existing_account() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let transactions = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            TransactionFilter::default(),
        )
        .unwrap();

        assert_eq!(
            transactions,
            vec![
                Transaction::new(
                    TransactionId::new(4),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(3),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(2),
                    account_id,
                    TransactionKind::Income,
                    Money::from_minor_units(2_000, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Salary"),
                    Category::Salary,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(1),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
                    "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Lunch"),
                    Category::Food,
                )
                .unwrap(),
            ]
        );
    }

    #[test]
    fn returns_error_for_nonexistent_account() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(999);
        let _ = account_repository
            .save(Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap());

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, AccountId::new(1));

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            TransactionFilter::default(),
        );
        assert_eq!(
            result,
            Err(ListTransactionsError::AccountNotFound(account_id))
        );
    }

    #[test]
    fn return_empty_list_for_account_with_no_transactions() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        account_repository
            .save(Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap())
            .unwrap();

        account_repository
            .save(Account::new(AccountId::new(2), String::from("Cash"), Currency::Cny).unwrap())
            .unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, AccountId::new(1));

        let transactions = list_account_transactions(
            &account_repository,
            transaction_repository,
            AccountId::new(2),
            TransactionFilter::default(),
        )
        .unwrap();

        assert_eq!(transactions.len(), 0);
    }

    struct FailingAccountRepository;

    impl AccountRepository for FailingAccountRepository {
        fn save(&mut self, _account: Account) -> Result<(), RepositoryError> {
            Ok(())
        }

        fn find_by_id(&self, _id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Err(RepositoryError::Storage(
                "account database unavailable".to_string(),
            ))
        }

        fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
            Err(RepositoryError::Storage(
                "account database unavailable".to_string(),
            ))
        }
    }

    #[test]
    fn returns_repository_error_when_loading_account_fails() {
        let account_repository = FailingAccountRepository;
        let transaction_repository = InMemoryTransactionRepository::new();

        let result = list_account_transactions(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            TransactionFilter::default(),
        );

        assert_eq!(
            result,
            Err(ListTransactionsError::Repository(RepositoryError::Storage(
                "account database unavailable".to_string(),
            ),)),
        );
    }

    struct FailingTransactionRepository;

    impl TransactionRepository for FailingTransactionRepository {
        fn save(&mut self, _transaction: Transaction) -> Result<(), RepositoryError> {
            Ok(())
        }

        fn find_by_account_id(
            &self,
            _account_id: AccountId,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage(
                "transaction database unavailable".to_string(),
            ))
        }
    }

    #[test]
    fn returns_repository_error_when_loading_transactions_fails() {
        let mut account_repository = InMemoryAccountRepository::new();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();

        account_repository.save(account).unwrap();

        let transaction_repository = FailingTransactionRepository;

        let result = list_account_transactions(
            &account_repository,
            &transaction_repository,
            account_id,
            TransactionFilter::default(),
        );

        assert_eq!(
            result,
            Err(ListTransactionsError::Repository(RepositoryError::Storage(
                "transaction database unavailable".to_string(),
            ),)),
        );
    }

    #[test]
    fn filters_transactions_by_category() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let filter = TransactionFilter {
            category: Some(Category::Food),
            kind: None,
            from: None,
            to: None,
            ..TransactionFilter::default()
        };

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            filter,
        );

        assert_eq!(
            result,
            Ok(vec![
                Transaction::new(
                    TransactionId::new(4),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(3),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(1),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
                    "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Lunch"),
                    Category::Food,
                )
                .unwrap(),
            ])
        );
    }

    #[test]
    fn filters_transactions_by_kind() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let filter = TransactionFilter {
            category: None,
            kind: Some(TransactionKind::Expense),
            from: None,
            to: None,
            ..TransactionFilter::default()
        };

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            filter,
        );

        assert_eq!(
            result,
            Ok(vec![
                Transaction::new(
                    TransactionId::new(4),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(3),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(1),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
                    "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Lunch"),
                    Category::Food,
                )
                .unwrap(),
            ])
        );
    }

    #[test]
    fn returns_empty_list_when_no_transactions_match_filter() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let filter = TransactionFilter {
            category: Some(Category::Entertainment),
            kind: Some(TransactionKind::Expense),
            from: None,
            to: None,
            ..TransactionFilter::default()
        };

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            filter,
        );

        assert_eq!(result, Ok(vec![]));
    }

    #[test]
    fn returns_error_when_from_date_is_after_to_date() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let filter = TransactionFilter {
            category: None,
            kind: Some(TransactionKind::Expense),
            from: Some("2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap()),
            to: Some("2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap()),
            ..TransactionFilter::default()
        };

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            filter,
        );

        assert_eq!(
            result,
            Err(ListTransactionsError::TimeRangeError {
                from: "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                to: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            })
        );
    }

    #[test]
    fn filters_transactions_by_time_range() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let filter = TransactionFilter {
            category: None,
            kind: Some(TransactionKind::Expense),
            from: Some("2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap()),
            to: Some("2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap()),
            ..TransactionFilter::default()
        };

        let result = list_account_transactions(
            &account_repository,
            transaction_repository,
            account_id,
            filter,
        );

        assert_eq!(
            result,
            Ok(vec![
                Transaction::new(
                    TransactionId::new(1),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
                    "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Lunch"),
                    Category::Food,
                )
                .unwrap(),
            ])
        );
    }

    #[test]
    fn filters_by_description_and_amount_range() {
        let (mut accounts, mut transactions) = in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        accounts
            .save(Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let transactions = build_sample_transactions(&mut transactions, account_id);

        let result = list_account_transactions(
            &accounts,
            transactions,
            account_id,
            TransactionFilter {
                description_contains: Some("DIN".to_string()),
                min_amount_minor: Some(400),
                max_amount_minor: Some(600),
                ..TransactionFilter::default()
            },
        )
        .unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|item| item.description() == "Dinner"));
    }

    #[test]
    fn rejects_invalid_search_and_page_ranges() {
        let (mut accounts, transactions) = in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        accounts
            .save(Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();

        assert_eq!(
            list_account_transactions(
                &accounts,
                &transactions,
                account_id,
                TransactionFilter {
                    min_amount_minor: Some(10),
                    max_amount_minor: Some(5),
                    ..TransactionFilter::default()
                },
            ),
            Err(ListTransactionsError::InvalidAmountRange {
                min: Some(10),
                max: Some(5),
            })
        );
        assert_eq!(
            list_account_transaction_page(
                &accounts,
                &transactions,
                account_id,
                TransactionFilter::default(),
                TransactionPageRequest {
                    limit: 0,
                    cursor: None,
                },
            ),
            Err(ListTransactionsError::InvalidPageLimit {
                limit: 0,
                max: MAX_TRANSACTION_PAGE_SIZE,
            })
        );
    }

    #[test]
    fn pages_with_stable_time_and_id_cursor() {
        let (mut accounts, mut transactions) = in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        accounts
            .save(Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        build_sample_transactions(&mut transactions, account_id);

        let first = list_account_transaction_page(
            &accounts,
            &transactions,
            account_id,
            TransactionFilter::default(),
            TransactionPageRequest {
                limit: 2,
                cursor: None,
            },
        )
        .unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|item| item.id().value())
                .collect::<Vec<_>>(),
            vec![4, 3]
        );

        let second = list_account_transaction_page(
            &accounts,
            &transactions,
            account_id,
            TransactionFilter::default(),
            TransactionPageRequest {
                limit: 2,
                cursor: first.next_cursor,
            },
        )
        .unwrap();
        assert_eq!(
            second
                .items
                .iter()
                .map(|item| item.id().value())
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(second.next_cursor, None);
    }
}
