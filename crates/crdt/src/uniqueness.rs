use core::types::{ColumnId, LooserEntry, RowId, TableId, UniquenessClaim, Version};
use std::collections::BTreeMap;

/// Manages uniqueness claims across tables/columns.
/// Implements the reservation/claim protocol for UNIQUE constraints.
///
/// Invariant: for a given (table, column, value), exactly one canonical owner exists.
/// Conflicting rows are preserved as losers, never silently deleted.
#[derive(Debug, Clone, Default)]
pub struct UniquenessRegistry {
    /// (table_id, column_id, value) -> claim
    claims: BTreeMap<(TableId, ColumnId, String), UniquenessClaim>,
}

impl UniquenessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to claim a unique value for a row.
    /// Returns Ok(()) if the claim wins or is a duplicate.
    /// The loser is recorded but not rejected (both rows survive internally).
    pub fn claim(
        &mut self,
        table_id: &str,
        column_id: &str,
        value: &str,
        row_id: &str,
        version: Version,
    ) -> ClaimResult {
        let key = (table_id.to_string(), column_id.to_string(), value.to_string());

        if let Some(existing) = self.claims.get_mut(&key) {
            if version > existing.version {
                // Challenger wins — demote current owner to loser
                let old_owner = LooserEntry {
                    row_id: existing.owner_row.clone(),
                    version: existing.version.clone(),
                };
                existing.losers.push(old_owner);
                // Also move prior losers' losers along
                existing.owner_row = row_id.to_string();
                existing.version = version;
                ClaimResult::Won
            } else if version == existing.version && row_id == existing.owner_row {
                // Idempotent re-claim
                ClaimResult::Won
            } else {
                // Incumbent wins — record challenger as loser
                existing.losers.push(LooserEntry {
                    row_id: row_id.to_string(),
                    version: version.clone(),
                });
                ClaimResult::Lost { winner_row: existing.owner_row.clone() }
            }
        } else {
            self.claims.insert(
                key,
                UniquenessClaim {
                    table_id: table_id.to_string(),
                    column_id: column_id.to_string(),
                    value: value.to_string(),
                    owner_row: row_id.to_string(),
                    version,
                    losers: vec![],
                },
            );
            ClaimResult::Won
        }
    }

    /// Get the canonical owner row for a unique value.
    pub fn owner(&self, table_id: &str, column_id: &str, value: &str) -> Option<&str> {
        let key = (table_id.to_string(), column_id.to_string(), value.to_string());
        self.claims.get(&key).map(|c| c.owner_row.as_str())
    }

    /// Check if a row is the canonical owner of a unique value.
    pub fn is_owner(&self, table_id: &str, column_id: &str, value: &str, row_id: &str) -> bool {
        self.owner(table_id, column_id, value).map_or(false, |o| o == row_id)
    }

    /// Merge in claims from another registry. Winner = higher version.
    pub fn merge(&mut self, other: &UniquenessRegistry) {
        for (key, other_claim) in &other.claims {
            if let Some(existing) = self.claims.get_mut(key) {
                if other_claim.version > existing.version {
                    // Merge losers from both sides
                    let mut all_losers = existing.losers.clone();
                    all_losers.extend(other_claim.losers.clone());
                    // Add old owner as loser
                    all_losers.push(LooserEntry {
                        row_id: existing.owner_row.clone(),
                        version: existing.version.clone(),
                    });
                    // Deduplicate
                    all_losers.sort_by(|a, b| a.row_id.cmp(&b.row_id));
                    all_losers.dedup_by(|a, b| a.row_id == b.row_id);
                    // Remove new winner from losers
                    all_losers.retain(|l| l.row_id != other_claim.owner_row);

                    existing.owner_row = other_claim.owner_row.clone();
                    existing.version = other_claim.version.clone();
                    existing.losers = all_losers;
                } else {
                    // Incumbent wins — add other's owner to losers if not already there
                    let already = existing.losers.iter().any(|l| l.row_id == other_claim.owner_row);
                    if !already && other_claim.owner_row != existing.owner_row {
                        existing.losers.push(LooserEntry {
                            row_id: other_claim.owner_row.clone(),
                            version: other_claim.version.clone(),
                        });
                        for l in &other_claim.losers {
                            if !existing.losers.iter().any(|e| e.row_id == l.row_id) {
                                existing.losers.push(l.clone());
                            }
                        }
                    }
                }
            } else {
                self.claims.insert(key.clone(), other_claim.clone());
            }
        }
    }

    pub fn all_claims(&self) -> impl Iterator<Item = &UniquenessClaim> {
        self.claims.values()
    }

    pub fn into_vec(self) -> Vec<UniquenessClaim> {
        self.claims.into_values().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimResult {
    Won,
    Lost { winner_row: RowId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::Version;

    #[test]
    fn first_claim_wins() {
        let mut reg = UniquenessRegistry::new();
        let v = Version::new(5, "peerA".to_string());
        let r = reg.claim("users", "email", "alice@x.com", "u1", v);
        assert_eq!(r, ClaimResult::Won);
        assert_eq!(reg.owner("users", "email", "alice@x.com"), Some("u1"));
    }

    #[test]
    fn higher_version_wins() {
        let mut reg = UniquenessRegistry::new();
        reg.claim("users", "email", "alice@x.com", "u1", Version::new(3, "peerA".to_string()));
        let r = reg.claim("users", "email", "alice@x.com", "u2", Version::new(7, "peerB".to_string()));
        assert_eq!(r, ClaimResult::Won);
        assert_eq!(reg.owner("users", "email", "alice@x.com"), Some("u2"));
    }

    #[test]
    fn loser_preserved() {
        let mut reg = UniquenessRegistry::new();
        reg.claim("users", "email", "alice@x.com", "u1", Version::new(7, "peerA".to_string()));
        let r = reg.claim("users", "email", "alice@x.com", "u2", Version::new(3, "peerB".to_string()));
        assert!(matches!(r, ClaimResult::Lost { .. }));
        let claim = reg.claims.get(&("users".to_string(), "email".to_string(), "alice@x.com".to_string())).unwrap();
        assert_eq!(claim.losers.len(), 1);
        assert_eq!(claim.losers[0].row_id, "u2");
    }

    #[test]
    fn idempotent_claim() {
        let mut reg = UniquenessRegistry::new();
        let v = Version::new(5, "peerA".to_string());
        reg.claim("users", "email", "alice@x.com", "u1", v.clone());
        let r = reg.claim("users", "email", "alice@x.com", "u1", v);
        assert_eq!(r, ClaimResult::Won);
    }
}
