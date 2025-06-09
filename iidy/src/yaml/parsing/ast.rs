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
    /// Plain string value (no handlebars templates)
    PlainString(String),
    /// Templated string value (contains handlebars templates)
    TemplatedString(String),
    /// YAML sequence (array)
    Sequence(Vec<YamlAst>),
    /// YAML mapping (object)
    Mapping(Vec<(YamlAst, YamlAst)>),
    /// Custom preprocessing tag
    PreprocessingTag(PreprocessingTag),
    /// CloudFormation intrinsic function (may contain YamlAst for preprocessing)
    CloudFormationTag(CloudFormationTag),
    /// Unknown YAML tag (for tags we don't recognize)
    UnknownYamlTag(UnknownTag),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnknownTag {
    pub tag: String,
    pub value: Box<YamlAst>
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
        match tag {
            "Ref" => Some(CloudFormationTag::Ref(Box::new(value))),
            "Sub" => Some(CloudFormationTag::Sub(Box::new(value))),
            "GetAtt" => Some(CloudFormationTag::GetAtt(Box::new(value))),
            "Join" => Some(CloudFormationTag::Join(Box::new(value))),
            "Select" => Some(CloudFormationTag::Select(Box::new(value))),
            "Split" => Some(CloudFormationTag::Split(Box::new(value))),
            "Base64" => Some(CloudFormationTag::Base64(Box::new(value))),
            "GetAZs" => Some(CloudFormationTag::GetAZs(Box::new(value))),
            "ImportValue" => Some(CloudFormationTag::ImportValue(Box::new(value))),
            "FindInMap" => Some(CloudFormationTag::FindInMap(Box::new(value))),
            "Cidr" => Some(CloudFormationTag::Cidr(Box::new(value))),
            "Length" => Some(CloudFormationTag::Length(Box::new(value))),
            "ToJsonString" => Some(CloudFormationTag::ToJsonString(Box::new(value))),
            "Transform" => Some(CloudFormationTag::Transform(Box::new(value))),
            "ForEach" => Some(CloudFormationTag::ForEach(Box::new(value))),
            "If" => Some(CloudFormationTag::If(Box::new(value))),
            "Equals" => Some(CloudFormationTag::Equals(Box::new(value))),
            "And" => Some(CloudFormationTag::And(Box::new(value))),
            "Or" => Some(CloudFormationTag::Or(Box::new(value))),
            "Not" => Some(CloudFormationTag::Not(Box::new(value))),
            _ => None,
        }
    }
    
    /// Get the tag name for this CloudFormation function
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
    
    /// Get the inner YamlAst value that needs preprocessing
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
            YamlAst::PlainString(s) | YamlAst::TemplatedString(s) => Some(Value::String(s.clone())),
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
            YamlAst::CloudFormationTag(_) => None, // CloudFormation tags need preprocessing
            YamlAst::UnknownYamlTag(_tag) => {
                // Unknown tags cannot be converted to plain values
                // They need to be preserved as-is in the YAML output
                None
            }
        }
    }
}
