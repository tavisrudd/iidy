//! AST definitions for YAML preprocessing with location tracking
//!
//! Defines the abstract syntax tree for YAML documents with custom preprocessing tags
//! and precise source location information for error reporting.

use serde_yaml::Number;
use url::Url;

// We'll define a simple Position type instead of using lsp_types for now
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SrcMeta {
    pub input_uri: Url,
    pub start: Position,
    pub end: Position,
}

impl SrcMeta {
    #[allow(dead_code)]
    pub fn new(uri: Url, start: Position, end: Position) -> Self {
        Self {
            input_uri: uri,
            start,
            end,
        }
    }
}

/// Main AST node for YAML with preprocessing support and location tracking
#[derive(Debug, Clone, PartialEq)]
pub enum YamlAst {
    /// Null value
    Null(SrcMeta),
    /// Boolean value
    Bool(bool, SrcMeta),
    /// Numeric value (preserves original integer/float representation)
    Number(Number, SrcMeta),
    /// Plain string value (no handlebars templates)
    PlainString(String, SrcMeta),
    /// Templated string value (contains handlebars templates)
    TemplatedString(String, SrcMeta),
    /// YAML sequence (array)
    Sequence(Vec<YamlAst>, SrcMeta),
    /// YAML mapping (object)
    Mapping(Vec<(YamlAst, YamlAst)>, SrcMeta),
    /// Custom preprocessing tag
    PreprocessingTag(PreprocessingTag, SrcMeta),
    /// CloudFormation intrinsic function (may contain YamlAst for preprocessing)
    CloudFormationTag(CloudFormationTag, SrcMeta),
    /// Unknown YAML tag (for tags we don't recognize)
    UnknownYamlTag(UnknownTag, SrcMeta),
    /// Imported document node (represents a document loaded from an external source)
    #[allow(dead_code)]
    ImportedDocument(ImportedDocumentNode, SrcMeta),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnknownTag {
    pub tag: String,
    pub value: Box<YamlAst>,
}

/// Represents an imported document within the AST
///
/// This node type allows tracking of imported documents during traversal,
/// maintaining the source URI and providing context for error reporting
/// and debugging during the import resolution process.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedDocumentNode {
    /// The source URI from which this document was imported
    pub source_uri: String,
    /// The key/alias under which this document was imported (from $imports)
    pub import_key: String,
    /// The resolved AST content of the imported document
    pub content: Box<YamlAst>,
    /// Metadata about the import operation
    pub metadata: ImportMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportMetadata {
    /// SHA256 hash of the imported content for integrity/caching
    pub content_hash: Option<String>,
    /// Timestamp when the import was resolved
    pub imported_at: Option<std::time::SystemTime>,
    /// The import type (file, s3, http, etc.)
    pub import_type: Option<String>,
}

/// CloudFormation intrinsic function tags that can contain YamlAst for preprocessing
///
/// These represent CloudFormation functions parsed from YAML that may still contain
/// preprocessing directives (handlebars templates, variable references, etc.)
/// that need to be resolved before converting to final CloudFormation expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum CloudFormationTag {
    /// !Ref - Reference to a parameter, resource, etc.
    Ref(Box<YamlAst>),
    /// !Sub - String substitution with CloudFormation variables
    Sub(Box<YamlAst>),
    /// !GetAtt - Get an attribute from a resource
    GetAtt(Box<YamlAst>),
    /// !Join - Join a list of values with a delimiter
    Join(Box<YamlAst>),
    /// !Select - Select an item from a list by index
    Select(Box<YamlAst>),
    /// !Split - Split a string into a list
    Split(Box<YamlAst>),
    /// !Base64 - Encode content as Base64
    Base64(Box<YamlAst>),
    /// !GetAZs - Get availability zones for a region
    GetAZs(Box<YamlAst>),
    /// !ImportValue - Import a value from another stack
    ImportValue(Box<YamlAst>),
    /// !FindInMap - Find a value in a mapping
    FindInMap(Box<YamlAst>),
    /// !Cidr - Generate CIDR blocks
    Cidr(Box<YamlAst>),
    /// !Length - Get the length of a list
    Length(Box<YamlAst>),
    /// !ToJsonString - Convert data to JSON string
    ToJsonString(Box<YamlAst>),
    /// !Transform - Apply a macro transformation
    Transform(Box<YamlAst>),
    /// !ForEach - Generate multiple resources
    ForEach(Box<YamlAst>),
    /// !If - Conditional evaluation
    If(Box<YamlAst>),
    /// !Equals - Test equality
    Equals(Box<YamlAst>),
    /// !And - Logical AND
    And(Box<YamlAst>),
    /// !Or - Logical OR
    Or(Box<YamlAst>),
    /// !Not - Logical NOT
    Not(Box<YamlAst>),
}

impl CloudFormationTag {
    /// Create a CloudFormation tag from a tag name and YamlAst value
    pub fn from_tag_name(tag: &str, value: YamlAst) -> Option<Self> {
        // CloudFormation tags should preserve the original structure, including arrays
        // Unlike preprocessing tags, they don't unwrap single-element arrays
        match tag {
            "Ref" | "!Ref" | "!Fn::Ref" => Some(CloudFormationTag::Ref(Box::new(value))),
            "Sub" | "!Sub" | "!Fn::Sub" => Some(CloudFormationTag::Sub(Box::new(value))),
            "GetAtt" | "!GetAtt" | "!Fn::GetAtt" => {
                Some(CloudFormationTag::GetAtt(Box::new(value)))
            }
            "Join" | "!Join" | "!Fn::Join" => Some(CloudFormationTag::Join(Box::new(value))),
            "Select" | "!Select" | "!Fn::Select" => {
                Some(CloudFormationTag::Select(Box::new(value)))
            }
            "Split" | "!Split" | "!Fn::Split" => Some(CloudFormationTag::Split(Box::new(value))),
            "Base64" | "!Base64" | "!Fn::Base64" => {
                Some(CloudFormationTag::Base64(Box::new(value)))
            }
            "GetAZs" | "!GetAZs" | "!Fn::GetAZs" => {
                Some(CloudFormationTag::GetAZs(Box::new(value)))
            }
            "ImportValue" | "!ImportValue" | "!Fn::ImportValue" => {
                Some(CloudFormationTag::ImportValue(Box::new(value)))
            }
            "FindInMap" | "!FindInMap" | "!Fn::FindInMap" => {
                Some(CloudFormationTag::FindInMap(Box::new(value)))
            }
            "Cidr" | "!Cidr" | "!Fn::Cidr" => Some(CloudFormationTag::Cidr(Box::new(value))),
            "Length" | "!Length" | "!Fn::Length" => {
                Some(CloudFormationTag::Length(Box::new(value)))
            }
            "ToJsonString" | "!ToJsonString" | "!Fn::ToJsonString" => {
                Some(CloudFormationTag::ToJsonString(Box::new(value)))
            }
            "Transform" | "!Transform" | "!Fn::Transform" => {
                Some(CloudFormationTag::Transform(Box::new(value)))
            }
            "ForEach" | "!ForEach" | "!Fn::ForEach" => {
                Some(CloudFormationTag::ForEach(Box::new(value)))
            }
            "If" | "!If" | "!Fn::If" => Some(CloudFormationTag::If(Box::new(value))),
            "Equals" | "!Equals" | "!Fn::Equals" => {
                Some(CloudFormationTag::Equals(Box::new(value)))
            }
            "And" | "!And" | "!Fn::And" => Some(CloudFormationTag::And(Box::new(value))),
            "Or" | "!Or" | "!Fn::Or" => Some(CloudFormationTag::Or(Box::new(value))),
            "Not" | "!Not" | "!Fn::Not" => Some(CloudFormationTag::Not(Box::new(value))),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn tag_name(&self) -> &'static str {
        match self {
            CloudFormationTag::Ref(_) => "Ref",
            CloudFormationTag::Sub(_) => "Sub",
            CloudFormationTag::GetAtt(_) => "GetAtt",
            CloudFormationTag::Join(_) => "Join",
            CloudFormationTag::Select(_) => "Select",
            CloudFormationTag::Split(_) => "Split",
            CloudFormationTag::Base64(_) => "Base64",
            CloudFormationTag::GetAZs(_) => "GetAZs",
            CloudFormationTag::ImportValue(_) => "ImportValue",
            CloudFormationTag::FindInMap(_) => "FindInMap",
            CloudFormationTag::Cidr(_) => "Cidr",
            CloudFormationTag::Length(_) => "Length",
            CloudFormationTag::ToJsonString(_) => "ToJsonString",
            CloudFormationTag::Transform(_) => "Transform",
            CloudFormationTag::ForEach(_) => "ForEach",
            CloudFormationTag::If(_) => "If",
            CloudFormationTag::Equals(_) => "Equals",
            CloudFormationTag::And(_) => "And",
            CloudFormationTag::Or(_) => "Or",
            CloudFormationTag::Not(_) => "Not",
        }
    }

    #[allow(dead_code)]
    pub fn inner_value(&self) -> &YamlAst {
        match self {
            CloudFormationTag::Ref(v) => v,
            CloudFormationTag::Sub(v) => v,
            CloudFormationTag::GetAtt(v) => v,
            CloudFormationTag::Join(v) => v,
            CloudFormationTag::Select(v) => v,
            CloudFormationTag::Split(v) => v,
            CloudFormationTag::Base64(v) => v,
            CloudFormationTag::GetAZs(v) => v,
            CloudFormationTag::ImportValue(v) => v,
            CloudFormationTag::FindInMap(v) => v,
            CloudFormationTag::Cidr(v) => v,
            CloudFormationTag::Length(v) => v,
            CloudFormationTag::ToJsonString(v) => v,
            CloudFormationTag::Transform(v) => v,
            CloudFormationTag::ForEach(v) => v,
            CloudFormationTag::If(v) => v,
            CloudFormationTag::Equals(v) => v,
            CloudFormationTag::And(v) => v,
            CloudFormationTag::Or(v) => v,
            CloudFormationTag::Not(v) => v,
        }
    }
}

/// All supported preprocessing tags in the iidy language
#[derive(Debug, Clone, PartialEq)]
pub enum PreprocessingTag {
    /// !$ - Variable lookup from environment (imports + defs).
    /// !$include is a deprecated alias.
    // TODO: rename IncludeTag to LookupTag
    Include(IncludeTag),
    /// !$if - Conditional logic
    If(IfTag),
    /// !$map - Transform lists/arrays
    Map(MapTag),
    /// !$merge - Combine mappings/objects
    Merge(MergeTag),
    /// !$concat - Merge sequences/arrays
    Concat(ConcatTag),
    /// !$let - Variable binding
    Let(LetTag),
    /// !$eq - Equality comparison
    Eq(EqTag),
    /// !$not - Boolean negation
    Not(NotTag),
    /// !$split - String to array conversion
    Split(SplitTag),
    /// !$join - Array to string conversion
    Join(JoinTag),
    /// !$concatMap - Map followed by concat
    ConcatMap(ConcatMapTag),
    /// !$mergeMap - Map followed by merge
    MergeMap(MergeMapTag),
    /// !$mapListToHash - Convert list of key-value pairs to hash
    MapListToHash(MapListToHashTag),
    /// !$mapValues - Transform object values while preserving keys
    MapValues(MapValuesTag),
    /// !$groupBy - Group items by key (like lodash groupBy)
    GroupBy(GroupByTag),
    /// !$fromPairs - Convert key-value pairs to object
    FromPairs(FromPairsTag),
    /// !$toYamlString - Convert data to YAML string
    ToYamlString(ToYamlStringTag),
    /// !$parseYaml - Parse YAML string back to data
    ParseYaml(ParseYamlTag),
    /// !$toJsonString - Convert data to JSON string
    ToJsonString(ToJsonStringTag),
    /// !$parseJson - Parse JSON string back to data
    ParseJson(ParseJsonTag),
    /// !$escape - Prevent preprocessing on child tree
    Escape(EscapeTag),
}

/// Variable lookup tag (!$ / !$include).
// TODO: rename to LookupTag
#[derive(Debug, Clone, PartialEq)]
pub struct IncludeTag {
    /// Dot-notation path to look up in the environment
    pub path: String,
    /// Optional comma-separated key filter
    pub query: Option<String>,
    /// Optional JMESPath expression (mutually exclusive with query)
    pub jmespath: Option<String>,
}

/// Conditional tag for if/then/else logic
#[derive(Debug, Clone, PartialEq)]
pub struct IfTag {
    /// Test condition to evaluate
    pub test: Box<YamlAst>,
    /// Value to use if condition is true
    pub then_value: Box<YamlAst>,
    /// Optional value to use if condition is false
    pub else_value: Option<Box<YamlAst>>,
}

/// Map transformation tag (matches iidy-js field names)
#[derive(Debug, Clone, PartialEq)]
pub struct MapTag {
    /// Items list/array to transform
    pub items: Box<YamlAst>,
    /// Template expression for transformation
    pub template: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var: Option<String>,
    /// Optional filter condition
    pub filter: Option<Box<YamlAst>>,
}

/// Merge tag for combining mappings
#[derive(Debug, Clone, PartialEq)]
pub struct MergeTag {
    /// List of mappings to merge
    pub sources: Vec<YamlAst>,
}

/// Concatenation tag for combining sequences
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatTag {
    /// List of sequences to concatenate
    pub sources: Vec<YamlAst>,
}

/// Variable binding tag (matches iidy-js flat format)
#[derive(Debug, Clone, PartialEq)]
pub struct LetTag {
    /// Variable bindings (key-value pairs) from flat structure
    pub bindings: Vec<(String, YamlAst)>,
    /// Expression to evaluate with bound variables (the "in" field)
    pub expression: Box<YamlAst>,
}

/// Equality comparison tag
#[derive(Debug, Clone, PartialEq)]
pub struct EqTag {
    /// Left side of comparison
    pub left: Box<YamlAst>,
    /// Right side of comparison
    pub right: Box<YamlAst>,
}

/// Boolean negation tag
#[derive(Debug, Clone, PartialEq)]
pub struct NotTag {
    /// Expression to negate
    pub expression: Box<YamlAst>,
}

/// String splitting tag (uses array format like iidy-js: [delimiter, string])
#[derive(Debug, Clone, PartialEq)]
pub struct SplitTag {
    /// Delimiter to split on
    pub delimiter: Box<YamlAst>,
    /// String to split
    pub string: Box<YamlAst>,
}

/// Array joining tag (takes [delimiter, array] format like iidy-js)
#[derive(Debug, Clone, PartialEq)]
pub struct JoinTag {
    /// Delimiter to join with
    pub delimiter: Box<YamlAst>,
    /// Array to join
    pub array: Box<YamlAst>,
}

/// ConcatMap tag for map followed by concat (matches iidy-js field names)
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMapTag {
    /// Items list/array to transform
    pub items: Box<YamlAst>,
    /// Template expression for transformation
    pub template: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var: Option<String>,
    /// Optional filter condition
    pub filter: Option<Box<YamlAst>>,
}

/// MergeMap tag for map followed by merge  
#[derive(Debug, Clone, PartialEq)]
pub struct MergeMapTag {
    /// Items list/array to transform
    pub items: Box<YamlAst>,
    /// Template expression for transformation
    pub template: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var: Option<String>,
}

/// MapListToHash tag for converting list of key-value pairs to hash (matches iidy-js field names)
#[derive(Debug, Clone, PartialEq)]
pub struct MapListToHashTag {
    /// Items list/array to transform
    pub items: Box<YamlAst>,
    /// Template expression for transformation
    pub template: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var: Option<String>,
    /// Optional filter condition
    pub filter: Option<Box<YamlAst>>,
}

/// MapValues tag for transforming object values while preserving keys (matches iidy-js field names)
#[derive(Debug, Clone, PartialEq)]
pub struct MapValuesTag {
    /// Items object to transform
    pub items: Box<YamlAst>,
    /// Template expression for transformation
    pub template: Box<YamlAst>,
    /// Optional variable name for current value (default: "item")
    pub var: Option<String>,
}

/// GroupBy tag for grouping items by key
#[derive(Debug, Clone, PartialEq)]
pub struct GroupByTag {
    /// Items list/array to group (matches iidy-js)
    pub items: Box<YamlAst>,
    /// Key expression or field name
    pub key: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var: Option<String>,
    /// Optional template for transforming grouped items
    pub template: Option<Box<YamlAst>>,
}

/// FromPairs tag for converting key-value pairs to object
#[derive(Debug, Clone, PartialEq)]
pub struct FromPairsTag {
    /// Source list of [key, value] pairs
    pub source: Box<YamlAst>,
}

/// ToYamlString tag for converting data to YAML string
#[derive(Debug, Clone, PartialEq)]
pub struct ToYamlStringTag {
    /// Data to convert to YAML string
    pub data: Box<YamlAst>,
}

/// ParseYaml tag for parsing YAML string back to data
#[derive(Debug, Clone, PartialEq)]
pub struct ParseYamlTag {
    /// YAML string to parse
    pub yaml_string: Box<YamlAst>,
}

/// ToJsonString tag for converting data to JSON string
#[derive(Debug, Clone, PartialEq)]
pub struct ToJsonStringTag {
    /// Data to convert to JSON string
    pub data: Box<YamlAst>,
}

/// ParseJson tag for parsing JSON string back to data
#[derive(Debug, Clone, PartialEq)]
pub struct ParseJsonTag {
    /// JSON string to parse
    pub json_string: Box<YamlAst>,
}

/// Escape tag for preventing preprocessing on child tree
#[derive(Debug, Clone, PartialEq)]
pub struct EscapeTag {
    /// Child tree to escape from preprocessing
    pub content: Box<YamlAst>,
}

impl YamlAst {
    #[allow(dead_code)]
    pub fn meta(&self) -> &SrcMeta {
        match self {
            YamlAst::Null(m)
            | YamlAst::Bool(_, m)
            | YamlAst::Number(_, m)
            | YamlAst::PlainString(_, m)
            | YamlAst::TemplatedString(_, m)
            | YamlAst::Sequence(_, m)
            | YamlAst::Mapping(_, m)
            | YamlAst::PreprocessingTag(_, m)
            | YamlAst::CloudFormationTag(_, m)
            | YamlAst::UnknownYamlTag(_, m)
            | YamlAst::ImportedDocument(_, m) => m,
        }
    }

    /// Get a human-readable type string for error reporting
    pub fn to_type_str(&self) -> &'static str {
        match self {
            YamlAst::Null(_) => "null",
            YamlAst::Bool(_, _) => "boolean",
            YamlAst::Number(_, _) => "number",
            YamlAst::PlainString(_, _) => "string",
            YamlAst::TemplatedString(_, _) => "templated string",
            YamlAst::Sequence(_, _) => "sequence",
            YamlAst::Mapping(_, _) => "object",
            YamlAst::PreprocessingTag(_, _) => "preprocessing tag",
            YamlAst::CloudFormationTag(_, _) => "CloudFormation tag",
            YamlAst::UnknownYamlTag(_, _) => "unknown tag",
            YamlAst::ImportedDocument(_, _) => "imported document",
        }
    }
}
