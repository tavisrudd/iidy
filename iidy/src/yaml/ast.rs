//! AST definitions for YAML preprocessing
//! 
//! Defines the abstract syntax tree for YAML documents with custom preprocessing tags

use serde_yaml::Value;

/// Main AST node for YAML with preprocessing support
#[derive(Debug, Clone, PartialEq)]
pub enum YamlAst {
    /// Regular scalar value (string, number, boolean, null)
    Scalar(String),
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

/// Array joining tag
#[derive(Debug, Clone, PartialEq)]
pub struct JoinTag {
    /// Array to join
    pub array: Box<YamlAst>,
    /// Delimiter to join with
    pub delimiter: String,
}

impl YamlAst {
    /// Check if this AST node represents a preprocessing tag
    pub fn is_preprocessing_tag(&self) -> bool {
        matches!(self, YamlAst::PreprocessingTag(_))
    }

    /// Convert to a standard YAML Value if possible (no preprocessing tags)
    pub fn to_value(&self) -> Option<Value> {
        match self {
            YamlAst::Scalar(s) => Some(Value::String(s.clone())),
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
            YamlAst::UnknownYamlTag(_) => todo!()
        }
    }
}
