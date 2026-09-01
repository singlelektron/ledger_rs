use crate::application::repository::{AuditLogRepository, RepositoryError};
use jiff::Timestamp;
use serde_json::Value;

pub const MAX_AUDIT_LOG_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditEntity {
    Account,
    Transaction,
    Transfer,
    Budget,
}

impl AuditEntity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Transaction => "transaction",
            Self::Transfer => "transfer",
            Self::Budget => "budget",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditOperation {
    Create,
    Update,
    Delete,
}

impl AuditOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditLogEntry {
    id: u64,
    changed_at: Timestamp,
    entity: AuditEntity,
    entity_id: u64,
    operation: AuditOperation,
    before_state: Option<Value>,
    after_state: Option<Value>,
}

impl AuditLogEntry {
    pub(crate) fn from_stored(
        id: u64,
        changed_at: Timestamp,
        entity: AuditEntity,
        entity_id: u64,
        operation: AuditOperation,
        before_state: Option<Value>,
        after_state: Option<Value>,
    ) -> Self {
        Self {
            id,
            changed_at,
            entity,
            entity_id,
            operation,
            before_state,
            after_state,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn changed_at(&self) -> Timestamp {
        self.changed_at
    }

    pub fn entity(&self) -> AuditEntity {
        self.entity
    }

    pub fn entity_id(&self) -> u64 {
        self.entity_id
    }

    pub fn operation(&self) -> AuditOperation {
        self.operation
    }

    pub fn before_state(&self) -> Option<&Value> {
        self.before_state.as_ref()
    }

    pub fn after_state(&self) -> Option<&Value> {
        self.after_state.as_ref()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ListAuditLogError {
    InvalidLimit { limit: usize, max: usize },
    Repository(RepositoryError),
}

impl From<RepositoryError> for ListAuditLogError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn list_recent_audit_entries(
    repository: &impl AuditLogRepository,
    limit: usize,
) -> Result<Vec<AuditLogEntry>, ListAuditLogError> {
    if !(1..=MAX_AUDIT_LOG_LIMIT).contains(&limit) {
        return Err(ListAuditLogError::InvalidLimit {
            limit,
            max: MAX_AUDIT_LOG_LIMIT,
        });
    }
    repository
        .list_recent(limit)
        .map_err(ListAuditLogError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnusedRepository;

    impl AuditLogRepository for UnusedRepository {
        fn list_recent(&self, _limit: usize) -> Result<Vec<AuditLogEntry>, RepositoryError> {
            unreachable!()
        }
    }

    #[test]
    fn rejects_zero_and_excessive_limits_before_querying_repository() {
        assert_eq!(
            list_recent_audit_entries(&UnusedRepository, 0),
            Err(ListAuditLogError::InvalidLimit {
                limit: 0,
                max: MAX_AUDIT_LOG_LIMIT,
            })
        );
        assert_eq!(
            list_recent_audit_entries(&UnusedRepository, MAX_AUDIT_LOG_LIMIT + 1),
            Err(ListAuditLogError::InvalidLimit {
                limit: MAX_AUDIT_LOG_LIMIT + 1,
                max: MAX_AUDIT_LOG_LIMIT,
            })
        );
    }
}
