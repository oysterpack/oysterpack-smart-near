use crate::Permissions;
use oysterpack_smart_near::asserts::ERR_INVALID;
use oysterpack_smart_near::Error;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContractPermissions(pub Option<HashMap<u8, &'static str>>);

impl ContractPermissions {
    /// retains permission bits that are in the range 0-61
    /// - 62 -> operator
    /// - 63 -> admin
    ///
    /// ## Panics
    /// - if permission names are not unique
    /// - if permission bits >= 62 were specified
    pub fn new(permissions: HashMap<u8, &'static str>) -> Self {
        let invalid_permissions: Vec<u8> = permissions
            .iter()
            .filter_map(|(k, _)| if *k >= 62_u8 { Some(*k) } else { None })
            .collect();
        assert!(
            invalid_permissions.is_empty(),
            "invalid permission bits were specified - the valid range is [0-61]"
        );
        let names: HashSet<_> = permissions.values().collect();
        ERR_INVALID.assert(
            || names.len() == permissions.len(),
            || "permission names must be unique",
        );
        Self(Some(permissions))
    }

    pub fn is_supported(&self, permissions: Permissions) -> bool {
        self.0.as_ref().map_or(false, |perms| {
            let supported_perms = perms
                .keys()
                .fold(0_u64, |supported_perms, perm| supported_perms | 1 << *perm);
            Permissions(supported_perms.into()).contains(permissions)
        })
    }

    /// permissions will be returned as sorted
    pub fn permission_names(&self, permissions: Permissions) -> Vec<String> {
        let mut labels = self.0.as_ref().map_or(vec![], |perms| {
            perms
                .keys()
                .filter(|perm| permissions.contains(1_u64 << *perm))
                .fold(vec![], |mut labels, perm| {
                    labels.push(perms.get(perm).as_ref().unwrap().to_string());
                    labels
                })
        });
        labels.sort();
        labels
    }

    /// unfolds the individual permissions from the specified `permissions` set. For example, if
    /// `permissions` has 5 permission bits set, then the 5 permissions will be extracted and returned.
    pub fn unfold_permissions(&self, permissions: Permissions) -> Vec<Permissions> {
        let mut perms = self.0.as_ref().map_or(vec![], |perms| {
            perms
                .keys()
                .filter(|perm| permissions.contains(1_u64 << *perm))
                .fold(vec![], |mut perms, perm| {
                    perms.push((1 << *perm).into());
                    perms
                })
        });
        perms.sort();
        perms
    }

    /// folds together the specified permissions specified by name into a [`Permissions']
    pub fn fold_permissions(&self, permissions: Vec<String>) -> Result<Permissions, Error<String>> {
        if permissions.is_empty() {
            return Ok(0.into());
        }

        match self.0.as_ref() {
            None => Err(ERR_INVALID.error("contract has no permissions".to_string())),
            Some(perms) => {
                let contract_perms: HashSet<String> =
                    perms.values().map(|perm| perm.to_string()).collect();
                let mut invalid_perms: Vec<String> = permissions
                    .iter()
                    .filter(|perm| {
                        // let perm = perm.to_string();
                        !contract_perms.contains(perm.as_str())
                    })
                    .map(|perm| perm.to_string())
                    .collect();
                if !invalid_perms.is_empty() {
                    invalid_perms.sort();
                    invalid_perms.dedup();
                    return Err(ERR_INVALID.error(format!(
                        "contract does not support specified permissions: {:?}",
                        invalid_perms
                    )));
                }
                let perms = perms.iter().fold(HashMap::new(), |mut perms, (k, v)| {
                    perms.insert(v.to_string(), *k);
                    perms
                });
                let permissions: u64 = permissions.iter().fold(0_64, |permissions, perm| {
                    permissions | 1 << *perms.get(perm).unwrap()
                });
                Ok(permissions.into())
            }
        }
    }
}

/// [`ContractPermissions`] can be constructed by specifying (permission_bit, permission_label) mappings:
///
/// ```rust
/// use oysterpack_smart_account_management::ContractPermissions;
/// let contract_permissions: ContractPermissions = vec![
///     (0, "PERM_0"),
///     (1, "PERM_1"),
///     (2, "PERM_2"),
/// ].into();
/// ```
///
/// ## Panics
/// - if any permission bits >= 62
impl From<Vec<(u8, &'static str)>> for ContractPermissions {
    fn from(values: Vec<(u8, &'static str)>) -> Self {
        let mut permissions = values
            .iter()
            .fold(HashMap::new(), |mut permissions, entry| {
                ERR_INVALID.assert(
                    || entry.0 < 62,
                    || "invalid permission bit - valid range is [0-61]",
                );
                permissions.insert(entry.0, entry.1);
                permissions
            });
        permissions.shrink_to_fit();
        ERR_INVALID.assert(
            || permissions.len() == values.len(),
            || "duplicate permission bits were specified",
        );
        ContractPermissions::new(permissions)
    }
}

#[cfg(test)]
mod test_contract_permissions {
    use super::*;
    use crate::Permission;
    use oysterpack_smart_near::near_sdk::test_utils;

    #[test]
    fn contract_permissions() {
        let contract_permissions = ContractPermissions::default();
        assert!(!contract_permissions.is_supported((1 << 15).into()));
        assert!(!contract_permissions.is_supported((1 << 0).into()));
        assert!(contract_permissions
            .permission_names((1 << 15).into())
            .is_empty());

        const MINTER: Permission = 1 << 10;
        const BURNER: Permission = 1 << 20;
        let mut perms = HashMap::new();
        perms.insert(10, "minter");
        perms.insert(20, "burner");
        let contract_permissions = ContractPermissions(Some(perms));

        assert!(!contract_permissions.is_supported((1 << 15).into()));
        assert!(contract_permissions.is_supported((1 << 10).into()));
        assert!(contract_permissions.is_supported(((1 << 10) | (1 << 20)).into()));
        assert!(!contract_permissions.is_supported(((1 << 10) | (1 << 15)).into()));

        let labels = contract_permissions.permission_names(((1 << 10) | (1 << 20)).into());
        println!("{:?}", labels);
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"minter".to_string()));
        assert!(labels.contains(&"burner".to_string()));

        let labels =
            contract_permissions.permission_names(((1 << 10) | (1 << 20) | (1 << 15)).into());
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"minter".to_string()));
        assert!(labels.contains(&"burner".to_string()));

        let perms = contract_permissions.unfold_permissions((MINTER | BURNER).into());
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&MINTER.into()));
        assert!(perms.contains(&BURNER.into()));
    }

    #[test]
    #[should_panic(expected = "[ERR] [INVALID] invalid permission bit - valid range is [0-61]")]
    fn create_with_invalid_bits() {
        test_utils::test_env::setup();
        let _contract_permissions: ContractPermissions = vec![(62_u8, "invalid")].into();
    }

    #[test]
    #[should_panic(expected = "[ERR] [INVALID] duplicate permission bits were specified")]
    fn create_with_duplicate_bits() {
        test_utils::test_env::setup();
        let _contract_permissions: ContractPermissions = vec![(1, "1"), (1, "1")].into();
    }

    #[test]
    #[should_panic(expected = "[ERR] [INVALID] permission names must be unique")]
    fn create_with_duplicate_perm_names() {
        test_utils::test_env::setup();
        let _contract_permissions: ContractPermissions = vec![(1, "1"), (2, "1")].into();
    }

    #[test]
    fn fold_permissions() {
        test_utils::test_env::setup();
        let contract_permissions: ContractPermissions = vec![(1, "1"), (2, "2"), (3, "3")].into();
        assert_eq!(
            contract_permissions.fold_permissions(vec![]).unwrap(),
            0.into()
        );
        let perm_123 = contract_permissions
            .fold_permissions(vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "3".to_string(),
            ])
            .unwrap();
        assert_eq!(
            contract_permissions.unfold_permissions(perm_123),
            vec![(1 << 1).into(), (1 << 2).into(), (1 << 3).into()]
        );
    }

    #[test]
    fn fold_permissions_with_no_contract_perms() {
        test_utils::test_env::setup();
        let contract_permissions: ContractPermissions = vec![].into();
        match contract_permissions.fold_permissions(vec![
            "1".to_string(),
            "3".to_string(),
            "2".to_string(),
        ]) {
            Ok(_) => panic!("should have failed"),
            Err(err) => {
                assert_eq!(err.0, ERR_INVALID);
                assert_eq!(
                    err.1,
                    "contract does not support specified permissions: [\"1\", \"2\", \"3\"]"
                );
            }
        }
    }
}
