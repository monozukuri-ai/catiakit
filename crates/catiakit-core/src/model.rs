use serde::{Deserialize, Serialize};

/// A recognized envelope does not establish the CATIA document kind; the root
/// storage declaration does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerFormat {
    CatiaV5Cfv2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Unknown,
    Part,
    Product,
    Drawing,
    Shape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub format: ContainerFormat,
    /// Derived from the root storage declaration; `Unknown` without a unique
    /// admitted declaration or for a declared type outside the mapped set.
    pub kind: DocumentKind,
    /// The declared type string as stored, such as `CATPart`, even when it
    /// maps to no `DocumentKind` variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<crate::container::ContainerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::metadata::MetadataInfo>,
}
