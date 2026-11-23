//! How much Git LFS storage a hosted project uses, as far as the provider's API tells it.
//!
//! - GitLab (gitlab.com and self-hosted): `GET /projects/:id?statistics=true` returns
//!   `statistics.lfs_objects_size`, `repository_size` and `storage_size` in bytes. Only members
//!   with the Reporter role or higher get the `statistics` object.
//!   Source: https://docs.gitlab.com/api/projects/ (read 2026-11-23).
//! - github.com: the billing usage summary of the repository owner,
//!   `GET /users/{user}/settings/billing/usage/summary` or
//!   `GET /organizations/{org}/settings/billing/usage/summary` (public preview, enhanced
//!   billing platform only; the organization one needs an organization admin). The docs do not
//!   name the Git LFS product / SKU, so every item whose product or SKU mentions "lfs" is kept.
//!   Source: https://docs.github.com/en/rest/billing/usage (read 2026-11-23).
//! - GitHub Enterprise Server, Gitea / Forgejo, Bitbucket and Azure DevOps: no API the app can
//!   read the LFS usage from ([`ProviderError::Unsupported`]).

use serde::{Deserialize, Serialize};

use crate::ProviderClient;
use crate::endpoint::ProviderKind;
use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;

/// LFS usage reported by the hosting provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum LfsServerUsage {
    /// GitLab project statistics, in bytes.
    #[serde(rename_all = "camelCase")]
    GitlabProject {
        /// LFS objects of the project.
        lfs_bytes: u64,
        /// The git repository itself.
        repository_bytes: u64,
        /// Everything the project stores (repository, LFS, artifacts, packages...).
        storage_bytes: u64,
    },
    /// GitHub billing usage of the repository owner for the current billing period.
    #[serde(rename_all = "camelCase")]
    GithubBilling {
        /// The account the usage is billed to.
        owner: String,
        organization: bool,
        /// Usage items about Git LFS (empty when none was billed this period).
        items: Vec<GithubUsageItem>,
    },
}

/// One Git LFS line of a GitHub usage summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GithubUsageItem {
    pub product: String,
    pub sku: String,
    /// As GitHub names it (for example gigabytes or gigabyte hours).
    pub unit_type: String,
    /// Total use in `unit_type`, included amount and overage together.
    pub quantity: f64,
    /// Part of `quantity` covered by the free allowance.
    pub included: f64,
    /// Amount billed after the allowance, in US dollars.
    pub net_amount: f64,
}

#[derive(Deserialize)]
struct RawGitlabProject {
    statistics: Option<RawGitlabStatistics>,
}

#[derive(Deserialize)]
struct RawGitlabStatistics {
    #[serde(default)]
    lfs_objects_size: u64,
    #[serde(default)]
    repository_size: u64,
    #[serde(default)]
    storage_size: u64,
}

#[derive(Deserialize)]
struct RawOwner {
    #[serde(rename = "type", default)]
    kind: String,
}

#[derive(Deserialize)]
struct RawUsageSummary {
    #[serde(rename = "usageItems", default)]
    usage_items: Vec<RawUsageItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUsageItem {
    #[serde(default)]
    product: String,
    #[serde(default)]
    sku: String,
    #[serde(default)]
    unit_type: String,
    #[serde(default)]
    gross_quantity: f64,
    #[serde(default)]
    discount_quantity: f64,
    #[serde(default)]
    net_amount: f64,
}

fn mentions_lfs(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    s.contains("lfs") || s.contains("large file")
}

/// Git LFS items of a usage summary body, the same product + SKU summed once.
pub fn github_lfs_items(body: &serde_json::Value) -> Result<Vec<GithubUsageItem>, ProviderError> {
    let summary: RawUsageSummary =
        serde_json::from_value(body.clone()).map_err(ProviderError::invalid_response)?;
    let mut out: Vec<GithubUsageItem> = Vec::new();
    for item in summary
        .usage_items
        .into_iter()
        .filter(|i| mentions_lfs(&i.product) || mentions_lfs(&i.sku))
    {
        if let Some(known) = out
            .iter_mut()
            .find(|o| o.product == item.product && o.sku == item.sku)
        {
            known.quantity += item.gross_quantity;
            known.included += item.discount_quantity;
            known.net_amount += item.net_amount;
        } else {
            out.push(GithubUsageItem {
                product: item.product,
                sku: item.sku,
                unit_type: item.unit_type,
                quantity: item.gross_quantity,
                included: item.discount_quantity,
                net_amount: item.net_amount,
            });
        }
    }
    Ok(out)
}

fn unsupported(message: &str) -> ProviderError {
    ProviderError::Unsupported {
        message: message.into(),
    }
}

impl ProviderClient {
    /// LFS usage of the hosted project `full_name` (`owner/repo`, `group/sub/project`).
    pub async fn lfs_usage(&self, full_name: &str) -> Result<LfsServerUsage, ProviderError> {
        match self {
            Self::Github(c) => c.lfs_usage(full_name).await,
            Self::Gitlab(c) => c.lfs_usage(full_name).await,
        }
    }
}

impl GithubClient {
    async fn lfs_usage(&self, full_name: &str) -> Result<LfsServerUsage, ProviderError> {
        if self.api().endpoint().kind() != ProviderKind::Github {
            return Err(unsupported(
                "only github.com reports Git LFS usage through its billing API",
            ));
        }
        let owner = full_name
            .trim_matches('/')
            .split('/')
            .next()
            .filter(|o| !o.is_empty())
            .ok_or_else(|| ProviderError::InvalidInput {
                message: format!("`{full_name}` is not an owner/repo name"),
            })?
            .to_owned();
        let kind = self
            .api()
            .get_json::<RawOwner>(&["users", &owner], &[])
            .await?
            .body
            .kind;
        let organization = kind.eq_ignore_ascii_case("organization");
        let scope = if organization {
            "organizations"
        } else {
            "users"
        };
        let body = self
            .api()
            .get_json::<serde_json::Value>(
                &[scope, &owner, "settings", "billing", "usage", "summary"],
                &[],
            )
            .await?
            .body;
        Ok(LfsServerUsage::GithubBilling {
            owner,
            organization,
            items: github_lfs_items(&body)?,
        })
    }
}

impl GitlabClient {
    async fn lfs_usage(&self, full_name: &str) -> Result<LfsServerUsage, ProviderError> {
        let path = full_name.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let project = self
            .api()
            .get_json::<RawGitlabProject>(&["projects", path], &[("statistics", "true")])
            .await?
            .body;
        let s = project.statistics.ok_or_else(|| ProviderError::Forbidden {
            message: "project statistics need the Reporter role or higher".into(),
            hint: None,
        })?;
        Ok(LfsServerUsage::GitlabProject {
            lfs_bytes: s.lfs_objects_size,
            repository_bytes: s.repository_size,
            storage_bytes: s.storage_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_lfs_items_and_sums_duplicates() {
        let body = serde_json::json!({
            "timePeriod": {"year": 2026, "month": 11},
            "usageItems": [
                {"product": "Actions", "sku": "actions_linux", "unitType": "Minutes",
                 "grossQuantity": 120.0, "discountQuantity": 120.0, "netAmount": 0.0},
                {"product": "Git LFS", "sku": "git_lfs_storage", "unitType": "GigabyteHours",
                 "grossQuantity": 10.5, "discountQuantity": 10.0, "netAmount": 0.04},
                {"product": "Git LFS", "sku": "git_lfs_storage", "unitType": "GigabyteHours",
                 "grossQuantity": 1.5, "discountQuantity": 0.0, "netAmount": 0.01},
                {"product": "git_lfs", "sku": "bandwidth", "unitType": "Gigabytes",
                 "grossQuantity": 2.0}
            ]
        });
        let items = github_lfs_items(&body).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].sku, "git_lfs_storage");
        assert!((items[0].quantity - 12.0).abs() < 1e-9);
        assert!((items[0].included - 10.0).abs() < 1e-9);
        assert!((items[0].net_amount - 0.05).abs() < 1e-9);
        assert_eq!(items[1].unit_type, "Gigabytes");
        assert!(github_lfs_items(&serde_json::json!({})).unwrap().is_empty());
    }

    #[test]
    fn usage_serializes_with_a_source_tag() {
        let v = serde_json::to_value(LfsServerUsage::GitlabProject {
            lfs_bytes: 1,
            repository_bytes: 2,
            storage_bytes: 3,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"source": "gitlabProject", "lfsBytes": 1, "repositoryBytes": 2, "storageBytes": 3})
        );
    }
}
