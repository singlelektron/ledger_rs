use crate::application::repository::{AccountRepository, BudgetRepository, RepositoryError};
use crate::domain::account::AccountId;
use crate::domain::budget::{Budget, BudgetError, BudgetId, BudgetMonth, NewBudget};
use crate::domain::money::Money;
use crate::domain::transaction::Category;

#[derive(Debug, PartialEq, Eq)]
pub enum ManageBudgetError {
    AccountNotFound(AccountId),
    BudgetNotFound(BudgetId),
    Budget(BudgetError),
    Repository(RepositoryError),
}

impl From<BudgetError> for ManageBudgetError {
    fn from(error: BudgetError) -> Self {
        Self::Budget(error)
    }
}

impl From<RepositoryError> for ManageBudgetError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn set_budget(
    accounts: &impl AccountRepository,
    budgets: &mut impl BudgetRepository,
    account_id: AccountId,
    category: Category,
    month: BudgetMonth,
    limit_minor: i64,
) -> Result<Budget, ManageBudgetError> {
    let account = accounts
        .find_by_id(account_id)?
        .ok_or(ManageBudgetError::AccountNotFound(account_id))?;
    let budget = NewBudget::new(
        account_id,
        category,
        month,
        Money::from_minor_units(limit_minor, account.currency()),
    )?;
    Ok(budgets.set(budget)?)
}

pub fn get_budget(
    budgets: &impl BudgetRepository,
    id: BudgetId,
) -> Result<Budget, ManageBudgetError> {
    budgets
        .find_by_id(id)?
        .ok_or(ManageBudgetError::BudgetNotFound(id))
}

pub fn list_budgets(
    accounts: &impl AccountRepository,
    budgets: &impl BudgetRepository,
    account_id: AccountId,
) -> Result<Vec<Budget>, ManageBudgetError> {
    if accounts.find_by_id(account_id)?.is_none() {
        return Err(ManageBudgetError::AccountNotFound(account_id));
    }
    let mut result = budgets.find_by_account_id(account_id)?;
    result.sort_by_key(|item| (item.month(), format!("{:?}", item.category())));
    Ok(result)
}

pub fn delete_budget(
    budgets: &mut impl BudgetRepository,
    id: BudgetId,
) -> Result<(), ManageBudgetError> {
    if !budgets.delete(id)? {
        return Err(ManageBudgetError::BudgetNotFound(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{AccountRepository, BudgetRepository};
    use crate::domain::account::NewAccount;
    use crate::domain::money::Currency;
    use crate::infrastructure::in_memory::{InMemoryAccountRepository, InMemoryBudgetRepository};

    #[test]
    fn sets_updates_lists_and_deletes_budget() {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut budgets = InMemoryBudgetRepository::new();
        let month = BudgetMonth::new(2026, 8).unwrap();
        let first = set_budget(
            &accounts,
            &mut budgets,
            account.id(),
            Category::Food,
            month,
            1000,
        )
        .unwrap();
        let updated = set_budget(
            &accounts,
            &mut budgets,
            account.id(),
            Category::Food,
            month,
            2000,
        )
        .unwrap();
        assert_eq!(first.id(), updated.id());
        assert_eq!(updated.limit().minor_units(), 2000);
        assert_eq!(
            list_budgets(&accounts, &budgets, account.id()).unwrap(),
            vec![updated]
        );
        delete_budget(&mut budgets, first.id()).unwrap();
        assert!(budgets.find_by_account_id(account.id()).unwrap().is_empty());
    }

    #[test]
    fn rejects_unknown_account_and_invalid_limit() {
        let accounts = InMemoryAccountRepository::new();
        let mut budgets = InMemoryBudgetRepository::new();
        let missing = AccountId::new(9);
        assert_eq!(
            set_budget(
                &accounts,
                &mut budgets,
                missing,
                Category::Food,
                BudgetMonth::new(2026, 8).unwrap(),
                100,
            ),
            Err(ManageBudgetError::AccountNotFound(missing))
        );
    }
}
