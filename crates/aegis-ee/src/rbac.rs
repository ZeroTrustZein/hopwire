//! Enterprise Role-Based Access Control (RBAC) & OIDC Authentication.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Auditor,
    Analyst,
    Investigator,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ViewAlerts,
    SubmitDisposition,
    ManageCases,
    GenerateSar,
    VerifyAudit,
    ManageConfig,
    SystemAdmin,
}

impl Role {
    pub fn has_permission(&self, perm: Permission) -> bool {
        match self {
            Role::Admin => true,
            Role::Investigator => matches!(
                perm,
                Permission::ViewAlerts
                    | Permission::SubmitDisposition
                    | Permission::ManageCases
                    | Permission::GenerateSar
                    | Permission::VerifyAudit
            ),
            Role::Analyst => matches!(
                perm,
                Permission::ViewAlerts | Permission::SubmitDisposition | Permission::VerifyAudit
            ),
            Role::Auditor => matches!(perm, Permission::ViewAlerts | Permission::VerifyAudit),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub sub: String,
    pub name: String,
    pub email: Option<String>,
    pub role: Role,
}

impl AuthUser {
    pub fn can(&self, perm: Permission) -> bool {
        self.role.has_permission(perm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_permissions_matrix() {
        let auditor = AuthUser {
            sub: "u1".into(),
            name: "Auditor".into(),
            email: None,
            role: Role::Auditor,
        };
        assert!(auditor.can(Permission::ViewAlerts));
        assert!(auditor.can(Permission::VerifyAudit));
        assert!(!auditor.can(Permission::SubmitDisposition));
        assert!(!auditor.can(Permission::GenerateSar));

        let investigator = AuthUser {
            sub: "u2".into(),
            name: "Lead".into(),
            email: None,
            role: Role::Investigator,
        };
        assert!(investigator.can(Permission::GenerateSar));
        assert!(investigator.can(Permission::ManageCases));
        assert!(!investigator.can(Permission::ManageConfig));

        let admin = AuthUser {
            sub: "u3".into(),
            name: "Admin".into(),
            email: None,
            role: Role::Admin,
        };
        assert!(admin.can(Permission::ManageConfig));
        assert!(admin.can(Permission::SystemAdmin));
    }
}
