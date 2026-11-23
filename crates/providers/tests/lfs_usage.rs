//! LFS usage from the provider APIs against a wiremock server (no real network).

use nanomgg_providers::lfs_usage::LfsServerUsage;
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer, kind: ProviderKind) -> ProviderClient {
    let endpoint = match kind {
        ProviderKind::GitlabSelfHosted => ProviderEndpoint::gitlab_self_hosted(&server.uri()),
        _ => ProviderEndpoint::custom(kind, &server.uri(), &format!("{}/api/v3", server.uri())),
    }
    .unwrap();
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("testtoken"),
    )
}

#[tokio::test]
async fn gitlab_reads_project_statistics() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/acme%2Fgame"))
        .and(query_param("statistics", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 7,
            "statistics": {
                "commit_count": 40, "storage_size": 3000, "repository_size": 1000,
                "lfs_objects_size": 1800, "job_artifacts_size": 200
            }
        })))
        .mount(&server)
        .await;
    let usage = client(&server, ProviderKind::GitlabSelfHosted)
        .lfs_usage("acme/game")
        .await
        .unwrap();
    assert_eq!(
        usage,
        LfsServerUsage::GitlabProject {
            lfs_bytes: 1800,
            repository_bytes: 1000,
            storage_bytes: 3000
        }
    );
}

#[tokio::test]
async fn gitlab_without_statistics_is_forbidden() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/acme%2Fgame"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 7})))
        .mount(&server)
        .await;
    let err = client(&server, ProviderKind::GitlabSelfHosted)
        .lfs_usage("acme/game")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");
}

#[tokio::test]
async fn github_reads_the_organization_billing_summary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/acme"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"login": "acme", "type": "Organization"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v3/organizations/acme/settings/billing/usage/summary",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "usageItems": [
                {"product": "Git LFS", "sku": "Git LFS Storage", "unitType": "GigabyteHours",
                 "grossQuantity": 4.0, "discountQuantity": 4.0, "netAmount": 0.0},
                {"product": "Copilot", "sku": "seats", "unitType": "UserMonths",
                 "grossQuantity": 1.0, "discountQuantity": 0.0, "netAmount": 19.0}
            ]
        })))
        .mount(&server)
        .await;
    let usage = client(&server, ProviderKind::Github)
        .lfs_usage("acme/game")
        .await
        .unwrap();
    let LfsServerUsage::GithubBilling {
        owner,
        organization,
        items,
    } = usage
    else {
        panic!("expected GitHub billing usage");
    };
    assert_eq!(owner, "acme");
    assert!(organization);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].sku, "Git LFS Storage");
}

#[tokio::test]
async fn github_enterprise_server_is_unsupported() {
    let server = MockServer::start().await;
    let err = client(&server, ProviderKind::GithubEnterprise)
        .lfs_usage("acme/game")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
}
