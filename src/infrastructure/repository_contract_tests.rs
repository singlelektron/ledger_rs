use crate::application::create_account::create_account;
use crate::application::list_transactions::{
    ListTransactionsError, MAX_TRANSACTION_PAGE_SIZE, TransactionFilter, TransactionPageRequest,
    list_account_transaction_page,
};
use crate::application::manage_account::{
    ManageAccountError, delete_account_with_dependencies, get_account, rename_account,
};
use crate::application::manage_budget::{delete_budget, set_budget};
use crate::application::manage_transaction::{
    ManageTransactionError, TransactionChanges, delete_transaction, get_transaction,
    update_transaction,
};
use crate::application::manage_transfer::{create_transfer, delete_transfer};
use crate::application::record_transaction::record_transaction;
use crate::application::repository::{
    AccountRepository, BudgetRepository, TransactionRepository, TransferRepository,
};
use crate::domain::account::AccountId;
use crate::domain::budget::BudgetMonth;
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{Category, NewTransaction, TransactionId, TransactionKind};
use crate::domain::transfer::NewTransfer;
use crate::infrastructure::in_memory::{
    InMemoryAccountRepository, InMemoryBudgetRepository, InMemoryTransactionRepository,
    InMemoryTransferRepository,
};
use crate::infrastructure::sqlite::open_complete_repositories;
use jiff::Zoned;

fn assert_complete_repository_contract(
    accounts: &mut impl AccountRepository,
    transactions: &mut impl TransactionRepository,
    transfers: &mut impl TransferRepository,
    budgets: &mut impl BudgetRepository,
) {
    let cash = create_account(accounts, "Cash".to_string(), Currency::Cny).unwrap();
    let bank = create_account(accounts, "Bank".to_string(), Currency::Cny).unwrap();
    assert_eq!(cash.id(), AccountId::new(1));
    assert_eq!(bank.id(), AccountId::new(2));
    let renamed = rename_account(accounts, cash.id(), "Wallet".to_string()).unwrap();
    assert_eq!(renamed.name(), "Wallet");
    assert_eq!(get_account(accounts, cash.id()).unwrap(), renamed);

    let occurred_at: Zoned = "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap();
    let mut recorded = Vec::new();
    for (amount, description) in [(100, "One"), (200, "Two"), (300, "Three")] {
        recorded.push(
            record_transaction(
                accounts,
                transactions,
                NewTransaction::new(
                    cash.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(amount, Currency::Cny),
                    occurred_at.clone(),
                    description.to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap(),
        );
    }
    let first_page = list_account_transaction_page(
        accounts,
        transactions,
        cash.id(),
        TransactionFilter::default(),
        TransactionPageRequest {
            limit: 2,
            cursor: None,
        },
    )
    .unwrap();
    assert_eq!(
        first_page
            .items
            .iter()
            .map(|value| value.id())
            .collect::<Vec<_>>(),
        vec![TransactionId::new(3), TransactionId::new(2)]
    );
    let second_page = list_account_transaction_page(
        accounts,
        transactions,
        cash.id(),
        TransactionFilter::default(),
        TransactionPageRequest {
            limit: 2,
            cursor: first_page.next_cursor,
        },
    )
    .unwrap();
    assert_eq!(second_page.items, vec![recorded[0].clone()]);
    assert_eq!(
        list_account_transaction_page(
            accounts,
            transactions,
            cash.id(),
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

    let updated = update_transaction(
        accounts,
        transactions,
        recorded[1].id(),
        TransactionChanges {
            description: Some("Updated".to_string()),
            ..TransactionChanges::default()
        },
    )
    .unwrap();
    assert_eq!(updated.description(), "Updated");
    delete_transaction(transactions, recorded[2].id()).unwrap();
    assert_eq!(
        get_transaction(transactions, recorded[2].id()),
        Err(ManageTransactionError::TransactionNotFound(
            recorded[2].id()
        ))
    );

    let transfer = create_transfer(
        accounts,
        transfers,
        NewTransfer::new(
            cash.id(),
            bank.id(),
            Money::from_minor_units(50, Currency::Cny),
            Money::from_minor_units(50, Currency::Cny),
            occurred_at,
            "Move".to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let month = BudgetMonth::new(2026, 8).unwrap();
    let first_budget =
        set_budget(accounts, budgets, cash.id(), Category::Food, month, 1000).unwrap();
    let updated_budget =
        set_budget(accounts, budgets, cash.id(), Category::Food, month, 2000).unwrap();
    assert_eq!(first_budget.id(), updated_budget.id());

    assert_eq!(
        delete_account_with_dependencies(accounts, transactions, transfers, budgets, cash.id()),
        Err(ManageAccountError::HasTransactions(cash.id()))
    );
    delete_transaction(transactions, recorded[0].id()).unwrap();
    delete_transaction(transactions, recorded[1].id()).unwrap();
    assert_eq!(
        delete_account_with_dependencies(accounts, transactions, transfers, budgets, cash.id()),
        Err(ManageAccountError::HasTransfers(cash.id()))
    );
    delete_transfer(transfers, transfer.id()).unwrap();
    assert_eq!(
        delete_account_with_dependencies(accounts, transactions, transfers, budgets, cash.id()),
        Err(ManageAccountError::HasBudgets(cash.id()))
    );
    delete_budget(budgets, first_budget.id()).unwrap();
    delete_account_with_dependencies(accounts, transactions, transfers, budgets, cash.id())
        .unwrap();
    assert_eq!(
        get_account(accounts, cash.id()),
        Err(ManageAccountError::AccountNotFound(cash.id()))
    );
    assert_eq!(
        delete_transaction(transactions, TransactionId::new(999)),
        Err(ManageTransactionError::TransactionNotFound(
            TransactionId::new(999)
        ))
    );
}

#[test]
fn in_memory_repositories_follow_complete_contract() {
    assert_complete_repository_contract(
        &mut InMemoryAccountRepository::new(),
        &mut InMemoryTransactionRepository::new(),
        &mut InMemoryTransferRepository::new(),
        &mut InMemoryBudgetRepository::new(),
    );
}

#[test]
fn sqlite_repositories_follow_complete_contract() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut accounts, mut transactions, mut transfers, mut budgets) =
        open_complete_repositories(temp_dir.path().join("contract.db")).unwrap();
    assert_complete_repository_contract(
        &mut accounts,
        &mut transactions,
        &mut transfers,
        &mut budgets,
    );
}
