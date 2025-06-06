//! AST definitions for YAML preprocessing
//! 
//! Defines the abstract syntax tree for YAML documents with custom preprocessing tags

use serde_yaml::Value;

/// Main AST node for YAML with preprocessing support
#[derive(Debug, Clone, PartialEq)]
pub enum YamlAst {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Numeric value (preserves original integer/float representation)
    Number(serde_yaml::Number),
    /// String value
    String(String),
    /// YAML sequence (array)
    Sequence(Vec<YamlAst>),
    /// YAML mapping (object)
    Mapping(Vec<(YamlAst, YamlAst)>),
    /// Custom preprocessing tag
    PreprocessingTag(PreprocessingTag),
    UnknownYamlTag(UnknownTag),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnknownTag {
    pub tag: String,
    pub value: Box<YamlAst>
}
/// All supported preprocessing tags in the iidy language
#[derive(Debug, Clone, PartialEq)]
pub enum PreprocessingTag {
    /// !$include or !$ - Include content from external file
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

/// Include tag for importing external content
#[derive(Debug, Clone, PartialEq)]
pub struct IncludeTag {
    /// Path or reference to include
    pub path: String,
    /// Optional query/selector for partial inclusion
    pub query: Option<String>,
}

/// Conditional tag for if/then/else logic
#[derive(Debug, Clone, PartialEq)]
pub struct IfTag {
    /// Condition to evaluate
    pub condition: Box<YamlAst>,
    /// Value to use if condition is true
    pub then_value: Box<YamlAst>,
    /// Optional value to use if condition is false
    pub else_value: Option<Box<YamlAst>>,
}

/// Map transformation tag
#[derive(Debug, Clone, PartialEq)]
pub struct MapTag {
    /// Source list/array to transform
    pub source: Box<YamlAst>,
    /// Transformation expression
    pub transform: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var_name: Option<String>,
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

/// Variable binding tag
#[derive(Debug, Clone, PartialEq)]
pub struct LetTag {
    /// Variable bindings
    pub bindings: Vec<(String, YamlAst)>,
    /// Expression to evaluate with bound variables
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

/// String splitting tag
#[derive(Debug, Clone, PartialEq)]
pub struct SplitTag {
    /// String to split
    pub string: Box<YamlAst>,
    /// Delimiter to split on
    pub delimiter: String,
}

/// Array joining tag (takes [delimiter, array] format like iidy-js)
#[derive(Debug, Clone, PartialEq)]
pub struct JoinTag {
    /// Delimiter to join with
    pub delimiter: Box<YamlAst>,
    /// Array to join
    pub array: Box<YamlAst>,
}

/// ConcatMap tag for map followed by concat
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMapTag {
    /// Source list/array to transform
    pub source: Box<YamlAst>,
    /// Transformation expression
    pub transform: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var_name: Option<String>,
}

/// MergeMap tag for map followed by merge  
#[derive(Debug, Clone, PartialEq)]
pub struct MergeMapTag {
    /// Source list/array to transform
    pub source: Box<YamlAst>,
    /// Transformation expression
    pub transform: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var_name: Option<String>,
}

/// MapListToHash tag for converting list of key-value pairs to hash
#[derive(Debug, Clone, PartialEq)]
pub struct MapListToHashTag {
    /// Source list of key-value pairs
    pub source: Box<YamlAst>,
    /// Key field name (default: "key")
    pub key_field: Option<String>,
    /// Value field name (default: "value")
    pub value_field: Option<String>,
}

/// MapValues tag for transforming object values while preserving keys
#[derive(Debug, Clone, PartialEq)]
pub struct MapValuesTag {
    /// Source object to transform
    pub source: Box<YamlAst>,
    /// Transformation expression
    pub transform: Box<YamlAst>,
    /// Optional variable name for current value (default: "value")
    pub var_name: Option<String>,
}

/// GroupBy tag for grouping items by key
#[derive(Debug, Clone, PartialEq)]
pub struct GroupByTag {
    /// Source list/array to group
    pub source: Box<YamlAst>,
    /// Key expression or field name
    pub key: Box<YamlAst>,
    /// Optional variable name for current item (default: "item")
    pub var_name: Option<String>,
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
    /// Check if this AST node represents a preprocessing tag
    pub fn is_preprocessing_tag(&self) -> bool {
        matches!(self, YamlAst::PreprocessingTag(_))
    }

    /// Convert to a standard YAML Value if possible (no preprocessing tags)
    pub fn to_value(&self) -> Option<Value> {
        match self {
            YamlAst::Null => Some(Value::Null),
            YamlAst::Bool(b) => Some(Value::Bool(*b)),
            YamlAst::Number(n) => Some(Value::Number(n.clone())),
            YamlAst::String(s) => Some(Value::String(s.clone())),
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    result.push(item.to_value()?);
                }
                Some(Value::Sequence(result))
            }
            YamlAst::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in map {
                    let key_val = key.to_value()?;
                    let value_val = value.to_value()?;
                    result.insert(key_val, value_val);
                }
                Some(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(_) => None, // Cannot convert preprocessing tags directly
            YamlAst::UnknownYamlTag(_tag) => {
                // Unknown tags (like !Ref, !Sub) cannot be converted to plain values
                // They need to be preserved as-is in the YAML output
                None
            }
        }
    }
}
