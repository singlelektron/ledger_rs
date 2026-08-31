use crate::{
    application::{
        account_balance::{GetAccountBalanceError, get_account_balance_with_transfers},
        list_accounts::{ListAccountsError, list_accounts},
        list_transactions::{ListTransactionsError, TransactionFilter, list_account_transactions},
        repository::{AccountRepository, TransactionRepository, TransferRepository},
    },
    domain::{account::Account, money::Money, transaction::Transaction},
};

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Accounts(ListAccountsError),
    Balance(GetAccountBalanceError),
    Transactions(ListTransactionsError),
}

impl From<ListAccountsError> for LoadError {
    fn from(error: ListAccountsError) -> Self {
        Self::Accounts(error)
    }
}

impl From<GetAccountBalanceError> for LoadError {
    fn from(error: GetAccountBalanceError) -> Self {
        Self::Balance(error)
    }
}

impl From<ListTransactionsError> for LoadError {
    fn from(error: ListTransactionsError) -> Self {
        Self::Transactions(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOverview {
    account: Account,
    balance: Money,
    transactions: Vec<Transaction>,
}

impl AccountOverview {
    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn balance(&self) -> &Money {
        &self.balance
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    accounts: Vec<AccountOverview>,
    selected: usize,
}

impl App {
    pub fn load(
        account_repository: &impl AccountRepository,
        transaction_repository: &impl TransactionRepository,
        transfer_repository: &impl TransferRepository,
    ) -> Result<Self, LoadError> {
        let accounts = list_accounts(account_repository)?
            .into_iter()
            .map(|account| {
                let balance = get_account_balance_with_transfers(
                    account_repository,
                    transaction_repository,
                    transfer_repository,
                    account.id(),
                )?;
                let transactions = list_account_transactions(
                    account_repository,
                    transaction_repository,
                    account.id(),
                    TransactionFilter::default(),
                )?;
                Ok(AccountOverview {
                    account,
                    balance,
                    transactions,
                })
            })
            .collect::<Result<Vec<_>, LoadError>>()?;

        Ok(Self {
            accounts,
            selected: 0,
        })
    }

    pub fn accounts(&self) -> &[AccountOverview] {
        &self.accounts
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.accounts.is_empty()).then_some(self.selected)
    }

    pub fn selected_account(&self) -> Option<&AccountOverview> {
        self.accounts.get(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.accounts.is_empty() {
            self.selected = (self.selected + 1) % self.accounts.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.accounts.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.accounts.len() - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::repository::{AccountRepository, TransactionRepository, TransferRepository},
        domain::{
            account::{Account, AccountId},
            money::{Currency, Money},
            transaction::{Category, Transaction, TransactionId, TransactionKind},
            transfer::NewTransfer,
        },
        infrastructure::in_memory::{
            InMemoryAccountRepository, InMemoryTransactionRepository, InMemoryTransferRepository,
        },
    };

    #[test]
    fn loads_accounts_balances_and_newest_first_transactions() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let cash = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let bank = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        accounts.save(bank.clone()).unwrap();
        accounts.save(cash.clone()).unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(1),
                    cash.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(1_000, Currency::Cny),
                    "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Salary".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(2),
                    cash.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(250, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Lunch".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(
                    cash.id(),
                    bank.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-31T11:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Move money".to_string(),
                )
                .unwrap(),
            )
            .unwrap();

        let app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.accounts()[0].account(), &cash);
        assert_eq!(
            app.accounts()[0].balance(),
            &Money::from_minor_units(650, Currency::Cny)
        );
        assert_eq!(app.accounts()[0].transactions()[0].id().value(), 2);
        assert_eq!(
            app.accounts()[1].balance(),
            &Money::from_minor_units(100, Currency::Cny)
        );
    }

    #[test]
    fn selection_wraps_and_empty_state_is_safe() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut empty = App::load(&accounts, &transactions, &transfers).unwrap();
        empty.select_next();
        empty.select_previous();
        assert_eq!(empty.selected_index(), None);

        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.select_previous();
        assert_eq!(app.selected_index(), Some(1));
        app.select_next();
        assert_eq!(app.selected_index(), Some(0));
    }
}
