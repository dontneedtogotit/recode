//! Contract Lock Service (ported from devcode `ContractLockService.ts`)
//!
//! Records and enforces cross-file contracts before and after code generation.
//! Contracts include: function signatures, schema fields, routes, error
//! formats, and authorization rules. Generated files are validated against the
//! lock after generation to ensure they respect the agreed-upon interfaces.
//!
//! This prevents the common failure mode where generated files compile but
//! violate implicit contracts (wrong parameter names, missing required fields,
//! inconsistent error shapes, etc.).
//!
//! Pure module — no I/O, no DI. The validation is heuristic-based (substring
//! matching), not a parser — it's a fast check that expected names appear in
//! the expected files, not a type checker.


use super::types::GeneratedFile;
use std::collections::HashMap;

// ─── Contract Types ─────────────────────────────────────────────────────────

/// A function/method signature contract.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignatureContract {
    /// Fully qualified name: `module.function` or `ClassName.method`
    pub id: String,
    /// Parameter names and types
    pub parameters: Vec<SignatureParameter>,
    /// Return type
    pub return_type: String,
    /// File where this signature is defined
    pub file: String,
    /// Line number where this signature is defined
    pub line: u32,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignatureParameter {
    pub name: String,
    pub param_type: String,
    pub required: bool,
}

/// A schema field contract (Zod, TypeScript interface, etc.).
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchemaFieldContract {
    pub schema_name: String,
    pub field_name: String,
    pub field_type: String,
    pub required: bool,
    /// Validation rules (e.g., "min: 1", "max: 255")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation: Vec<String>,
    pub file: String,
}

/// A route contract.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RouteContract {
    pub method: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<String>,
    pub response_schema: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    pub file: String,
}

/// An error format contract.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorFormatContract {
    pub code: String,
    pub status_code: u16,
    pub message_pattern: String,
    pub has_details: bool,
    pub file: String,
}

/// An authorization rule contract.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuthorizationContract {
    pub resource: String,
    pub actions: Vec<String>,
    pub required_roles: Vec<String>,
    pub deny_by_default: bool,
    pub file: String,
}

/// Complete contract lock for a project.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractLock {
    pub id: String,
    pub created_at: u64,
    pub signatures: Vec<SignatureContract>,
    pub schema_fields: Vec<SchemaFieldContract>,
    pub routes: Vec<RouteContract>,
    pub error_formats: Vec<ErrorFormatContract>,
    pub authorization_rules: Vec<AuthorizationContract>,
}

/// A contract violation found during validation.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractViolation {
    pub violation_type: String,
    pub contract_id: String,
    pub description: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Validation result for a single generated file against the contract lock.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractValidationResult {
    pub file: String,
    pub passed: bool,
    pub violations: Vec<ContractViolation>,
}

/// Inputs for creating a contract lock.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
pub struct ContractLockInput {
    pub signatures: Vec<SignatureContract>,
    pub schema_fields: Vec<SchemaFieldContract>,
    pub routes: Vec<RouteContract>,
    pub error_formats: Vec<ErrorFormatContract>,
    pub authorization_rules: Vec<AuthorizationContract>,
}

// ─── Service ───────────────────────────────────────────────────────────────

/// Contract lock service: records and enforces cross-file contracts.
///
/// Pure domain logic — stores locks in an in-memory HashMap, validates
/// generated files by checking that expected names appear in the expected
/// files. The checks are heuristic (substring matching), not parser-based:
/// they catch the common failure where a file is generated without the
/// expected export/field/route, not subtle type mismatches.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
pub struct ContractLockService {
    locks: HashMap<String, ContractLock>,
}

impl Default for ContractLockService {
    fn default() -> Self {
        Self { locks: HashMap::new() }
    }
}

impl ContractLockService {
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new contract lock from extracted contracts.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn create_lock(&mut self, input: ContractLockInput) -> ContractLock {
        let id = format!(
            "lock-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let lock = ContractLock {
            id: id.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            signatures: input.signatures,
            schema_fields: input.schema_fields,
            routes: input.routes,
            error_formats: input.error_formats,
            authorization_rules: input.authorization_rules,
        };
        self.locks.insert(id, lock.clone());
        lock
    }

    /// Validate generated files against a contract lock.
    /// Returns a list of validation results, one per file.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn validate_against_lock(
        &self,
        files: &[GeneratedFile],
        lock: &ContractLock,
    ) -> Vec<ContractValidationResult> {
        files
            .iter()
            .map(|file| {
                let violations = self.validate_file(file, lock);
                ContractValidationResult {
                    file: file.path.clone(),
                    passed: violations.is_empty(),
                    violations,
                }
            })
            .collect()
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    fn validate_file(
        &self,
        file: &GeneratedFile,
        lock: &ContractLock,
    ) -> Vec<ContractViolation> {
        let mut violations = Vec::new();
        let content = &file.content;

        // Check signatures — only for the file where the signature is declared.
        for signature in &lock.signatures {
            if signature.file == file.path {
                let function_name = signature.id.split('.').last().unwrap_or(&signature.id);
                if !content.contains(function_name) {
                    violations.push(ContractViolation {
                        violation_type: "missing_signature".into(),
                        contract_id: signature.id.clone(),
                        description: format!(
                            "Missing signature: {} in {}",
                            signature.id, file.path
                        ),
                        file: file.path.clone(),
                        line: Some(signature.line),
                    });
                }
            }
        }

        // Check schema fields.
        for field in &lock.schema_fields {
            if field.file == file.path {
                if !content.contains(&field.field_name) {
                    violations.push(ContractViolation {
                        violation_type: "missing_schema_field".into(),
                        contract_id: format!("{}.{}", field.schema_name, field.field_name),
                        description: format!(
                            "Missing schema field: {}.{} in {}",
                            field.schema_name, field.field_name, file.path
                        ),
                        file: file.path.clone(),
                        line: None,
                    });
                }
            }
        }

        // Check routes.
        for route in &lock.routes {
            if route.file == file.path {
                let method_lower = route.method.to_lowercase();
                if !content.contains(&route.path) && !content.contains(&method_lower) {
                    violations.push(ContractViolation {
                        violation_type: "missing_route".into(),
                        contract_id: format!("{} {}", route.method, route.path),
                        description: format!(
                            "Missing route: {} {} in {}",
                            route.method, route.path, file.path
                        ),
                        file: file.path.clone(),
                        line: None,
                    });
                }
            }
        }

        // Check error formats.
        for error_format in &lock.error_formats {
            if error_format.file == file.path {
                if !content.contains(&error_format.code) {
                    violations.push(ContractViolation {
                        violation_type: "missing_error_format".into(),
                        contract_id: error_format.code.clone(),
                        description: format!(
                            "Missing error format: {} in {}",
                            error_format.code, file.path
                        ),
                        file: file.path.clone(),
                        line: None,
                    });
                }
            }
        }

        // Check authorization rules.
        for auth_rule in &lock.authorization_rules {
            if auth_rule.file == file.path {
                if !content.contains(&auth_rule.resource) {
                    violations.push(ContractViolation {
                        violation_type: "missing_auth_rule".into(),
                        contract_id: auth_rule.resource.clone(),
                        description: format!(
                            "Missing authorization rule for {} in {}",
                            auth_rule.resource, file.path
                        ),
                        file: file.path.clone(),
                        line: None,
                    });
                }
            }
        }

        violations
    }

    /// Get a lock by ID.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn get_lock(&self, id: &str) -> Option<&ContractLock> {
        self.locks.get(id)
    }

    /// Remove a lock by ID.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn remove_lock(&mut self, id: &str) -> bool {
        self.locks.remove(id).is_some()
    }

    /// Clear all locks.
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn clear(&mut self) {
        self.locks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::types::{GeneratedFile, GeneratedFileStatus};

    fn file(path: &str, content: &str) -> GeneratedFile {
        GeneratedFile {
            path: path.into(),
            content: content.into(),
            language: Some("typescript".into()),
            status: GeneratedFileStatus::Written,
            error: None,
        }
    }

    #[test]
    fn create_lock_stores_contracts() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![SignatureContract {
                id: "mod.createUser".into(),
                parameters: vec![SignatureParameter {
                    name: "name".into(),
                    param_type: "string".into(),
                    required: true,
                }],
                return_type: "User".into(),
                file: "src/services/userService.ts".into(),
                line: 10,
            }],
            schema_fields: vec![SchemaFieldContract {
                schema_name: "User".into(),
                field_name: "email".into(),
                field_type: "string".into(),
                required: true,
                validation: vec![],
                file: "src/types.ts".into(),
            }],
            routes: vec![RouteContract {
                method: "POST".into(),
                path: "/api/users".into(),
                request_schema: Some("CreateUserRequest".into()),
                response_schema: "User".into(),
                permissions: vec![],
                file: "src/routes/users.ts".into(),
            }],
            error_formats: vec![ErrorFormatContract {
                code: "USER_NOT_FOUND".into(),
                status_code: 404,
                message_pattern: "User {id} not found".into(),
                has_details: true,
                file: "src/errors.ts".into(),
            }],
            authorization_rules: vec![AuthorizationContract {
                resource: "users".into(),
                actions: vec!["delete".into()],
                required_roles: vec!["admin".into()],
                deny_by_default: true,
                file: "src/auth.ts".into(),
            }],
        });
        assert!(!lock.id.is_empty());
        assert_eq!(lock.signatures.len(), 1);
        assert_eq!(lock.schema_fields.len(), 1);
        assert_eq!(lock.routes.len(), 1);
        assert_eq!(lock.error_formats.len(), 1);
        assert_eq!(lock.authorization_rules.len(), 1);
        assert!(service.get_lock(&lock.id).is_some());
    }

    #[test]
    fn validate_passes_when_all_contracts_present() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![SignatureContract {
                id: "mod.createUser".into(),
                parameters: vec![],
                return_type: "User".into(),
                file: "src/services/userService.ts".into(),
                line: 10,
            }],
            schema_fields: vec![SchemaFieldContract {
                schema_name: "User".into(),
                field_name: "email".into(),
                field_type: "string".into(),
                required: true,
                validation: vec![],
                file: "src/types.ts".into(),
            }],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });

        let files = vec![
            file("src/services/userService.ts", "export function createUser(name: string): User { ... }"),
            file("src/types.ts", "export interface User { email: string; }"),
        ];

        let results = service.validate_against_lock(&files, &lock);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed), "{results:?}");
    }

    #[test]
    fn validate_detects_missing_signature() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![SignatureContract {
                id: "mod.createUser".into(),
                parameters: vec![],
                return_type: "User".into(),
                file: "src/services/userService.ts".into(),
                line: 10,
            }],
            schema_fields: vec![],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });

        // File exists but doesn't contain the function name.
        let files = vec![file("src/services/userService.ts", "export function deleteUser() {}")];

        let results = service.validate_against_lock(&files, &lock);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert_eq!(results[0].violations.len(), 1);
        assert_eq!(results[0].violations[0].violation_type, "missing_signature");
        assert_eq!(results[0].violations[0].contract_id, "mod.createUser");
    }

    #[test]
    fn validate_detects_missing_schema_field() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![],
            schema_fields: vec![SchemaFieldContract {
                schema_name: "User".into(),
                field_name: "email".into(),
                field_type: "string".into(),
                required: true,
                validation: vec![],
                file: "src/types.ts".into(),
            }],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });

        let files = vec![file("src/types.ts", "export interface User { name: string; }")];

        let results = service.validate_against_lock(&files, &lock);
        assert!(!results[0].passed);
        assert_eq!(results[0].violations[0].violation_type, "missing_schema_field");
    }

    #[test]
    fn validate_detects_missing_route() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![],
            schema_fields: vec![],
            routes: vec![RouteContract {
                method: "POST".into(),
                path: "/api/users".into(),
                request_schema: None,
                response_schema: "User".into(),
                permissions: vec![],
                file: "src/routes/users.ts".into(),
            }],
            error_formats: vec![],
            authorization_rules: vec![],
        });

        let files = vec![file("src/routes/users.ts", "export function getUsers() {}")];

        let results = service.validate_against_lock(&files, &lock);
        assert!(!results[0].passed);
        assert_eq!(results[0].violations[0].violation_type, "missing_route");
    }

    #[test]
    fn validate_only_checks_contracts_for_their_declared_file() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![SignatureContract {
                id: "mod.createUser".into(),
                parameters: vec![],
                return_type: "User".into(),
                file: "src/services/userService.ts".into(),
                line: 10,
            }],
            schema_fields: vec![],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });

        // A different file that doesn't contain createUser should NOT trigger
        // a violation — the contract is scoped to its declared file.
        let files = vec![file("src/App.tsx", "export function App() { return null; }")];

        let results = service.validate_against_lock(&files, &lock);
        assert!(results[0].passed, "contract should only check its declared file");
    }

    #[test]
    fn remove_and_clear_locks() {
        let mut service = ContractLockService::new();
        let lock = service.create_lock(ContractLockInput {
            signatures: vec![],
            schema_fields: vec![],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });
        assert!(service.get_lock(&lock.id).is_some());
        assert!(service.remove_lock(&lock.id));
        assert!(service.get_lock(&lock.id).is_none());

        let lock2 = service.create_lock(ContractLockInput {
            signatures: vec![],
            schema_fields: vec![],
            routes: vec![],
            error_formats: vec![],
            authorization_rules: vec![],
        });
        service.clear();
        assert!(service.get_lock(&lock2.id).is_none());
    }
}
