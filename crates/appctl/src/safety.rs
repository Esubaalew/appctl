use anyhow::{Result, bail};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use serde_json::Value;

use crate::schema::{Action, Provenance, Safety, Transport};

#[derive(Debug, Clone, Copy)]
pub struct SafetyMode {
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    /// Refuse tools whose HTTP surface was guessed (not OpenAPI / doctor-verified).
    pub strict: bool,
}

impl SafetyMode {
    pub fn check(&self, action: &Action, arguments: &Value) -> Result<()> {
        if self.strict
            && matches!(action.transport, Transport::Http { .. })
            && action.provenance == Provenance::Inferred
        {
            bail!(
                "tool '{}' uses an inferred HTTP route; run `appctl doctor --write` or drop --strict",
                action.name
            );
        }

        if self.read_only && action.safety != Safety::ReadOnly {
            bail!("action '{}' blocked in read-only mode", action.name);
        }

        if self.confirm {
            return Ok(());
        }

        match action.safety {
            Safety::ReadOnly => Ok(()),
            Safety::Mutating => {
                let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Execute '{}' with payload {}?",
                        action.name,
                        serde_json::to_string_pretty(arguments)?
                    ))
                    .default(false)
                    .interact()?;
                if confirmed {
                    Ok(())
                } else {
                    bail!("operation cancelled")
                }
            }
            Safety::Destructive => {
                let confirmation: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!("Type 'delete' to confirm '{}'", action.name))
                    .interact_text()?;
                if confirmation == "delete" {
                    Ok(())
                } else {
                    bail!("operation cancelled")
                }
            }
        }
    }
}
