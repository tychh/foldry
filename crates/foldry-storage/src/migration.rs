use serde_json::Value;

/// Public YAML document family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentKind {
    Plan,
    Settings,
}

impl DocumentKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Settings => "settings",
        }
    }
}

/// One deterministic major-version migration.
#[derive(Clone, Copy)]
pub struct MigrationStep {
    pub from: u16,
    pub to: u16,
    pub migrate: fn(Value) -> Result<Value, String>,
}

/// Sequential migration gateway. The initial v1 schema intentionally has no
/// predecessor, but all future versions must enter through this chain.
pub struct MigrationRegistry {
    kind: DocumentKind,
    current: u16,
    steps: Vec<MigrationStep>,
}

impl MigrationRegistry {
    #[must_use]
    pub fn new(kind: DocumentKind, current: u16, steps: Vec<MigrationStep>) -> Self {
        Self {
            kind,
            current,
            steps,
        }
    }

    #[must_use]
    pub const fn current(&self) -> u16 {
        self.current
    }

    pub fn migrate(&self, mut version: u16, mut value: Value) -> Result<Value, String> {
        if version > self.current {
            return Err(format!(
                "{} version {version} is newer than supported version {}",
                self.kind.name(),
                self.current
            ));
        }

        while version < self.current {
            let step = self
                .steps
                .iter()
                .find(|step| step.from == version && step.to == version + 1)
                .ok_or_else(|| {
                    format!(
                        "no {} migration from version {version} to {}",
                        self.kind.name(),
                        version + 1
                    )
                })?;
            value = (step.migrate)(value)?;
            version = step.to;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn migrate_zero_to_one(mut value: Value) -> Result<Value, String> {
        value["version"] = json!(1);
        value["migrated"] = json!(true);
        Ok(value)
    }

    #[test]
    fn applies_migrations_sequentially() {
        let registry = MigrationRegistry::new(
            DocumentKind::Plan,
            1,
            vec![MigrationStep {
                from: 0,
                to: 1,
                migrate: migrate_zero_to_one,
            }],
        );

        let migrated = registry.migrate(0, json!({"version": 0})).unwrap();

        assert_eq!(migrated["version"], 1);
        assert_eq!(migrated["migrated"], true);
    }

    #[test]
    fn refuses_a_gap_in_the_migration_chain() {
        let registry = MigrationRegistry::new(DocumentKind::Settings, 2, Vec::new());

        assert!(registry.migrate(1, json!({"version": 1})).is_err());
    }
}
