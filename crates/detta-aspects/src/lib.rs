use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const ASPECT_SOURCE_SCHEMA: &str = "detta.aspect-source.v1";
pub const ASPECT_SOURCE_SCHEMA_VERSION: u32 = 1;
pub const ASPECT_LANGUAGE: &str = "detta-aspect-metta.v1";
pub const TAXONOMY_VERSION_NORMALIZED_BALANCE_FIRST: &str = "NormalizedBalanceFirst.v1";

const FORBIDDEN_FORMS: &[&str] = &[
    "add-atom",
    "remove-atom",
    "get-atoms",
    "match",
    "import!",
    "py-call",
    "callPredicate",
    "assertzPredicate",
    "assertaPredicate",
    "retractPredicate",
    "translatePredicate",
    "add-translator-rule!",
    "remove-translator-rule!",
    "filesystem-access",
    "process-execution",
    "network-access",
    "wall-clock-access",
    "randomness",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectPackageAst {
    pub declarations: Vec<AspectDeclaration>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AspectDeclaration {
    Type {
        symbol: String,
        type_expr: Expr,
    },
    Aspect {
        aspect: String,
    },
    AbstractAspect {
        aspect: String,
    },
    Bundle {
        bundle: String,
    },
    LayerOf {
        aspect: String,
        layer: String,
    },
    Extends {
        aspect: String,
        parent: String,
    },
    Conflicts {
        aspect: String,
        other: String,
    },
    Owns {
        aspect: String,
        state: String,
        type_expr: Expr,
    },
    RegistryOwns {
        aspect: String,
        registry: String,
        type_expr: Expr,
    },
    Provides {
        aspect: String,
        facet: String,
    },
    Requires {
        aspect: String,
        facet: String,
    },
    Action {
        aspect: String,
        action: String,
    },
    Derived {
        aspect: String,
        action: String,
        expr: Expr,
    },
    LocalInvariant {
        aspect: String,
        invariant: String,
        expr: Expr,
    },
    BundleIncludes {
        bundle: String,
        aspect: String,
    },
    BundleExtends {
        bundle: String,
        parent: String,
    },
    BundleConstraint {
        bundle: String,
        constraint: String,
        expr: Expr,
    },
    Projection {
        bundle: String,
        projection: String,
        expr: Expr,
    },
    MethodAbi {
        bundle: String,
        projection: String,
        args: Expr,
        return_type: Expr,
    },
    MethodPolicy {
        bundle: String,
        projection: String,
        authority: Expr,
        effects: Expr,
        invariants: Expr,
    },
    CrossConstraint {
        constraint: String,
        expr: Expr,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Expr {
    Atom(String),
    List(Vec<Expr>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectModuleIr {
    pub taxonomy_version: String,
    pub source_root: String,
    pub types: BTreeMap<String, Expr>,
    pub aspects: BTreeMap<String, AspectDef>,
    pub bundles: BTreeMap<String, BundleDef>,
    pub projections: BTreeMap<String, ProjectionDef>,
    pub actions: BTreeMap<String, ActionDef>,
    pub storage_schema: BTreeMap<String, StateSchema>,
    pub registry_schema: BTreeMap<String, RegistrySchema>,
    pub invariants: BTreeMap<String, InvariantDef>,
    pub abi: BTreeMap<String, MethodAbiDef>,
    pub policies: BTreeMap<String, MethodPolicyIr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectDef {
    pub aspect: String,
    pub is_abstract: bool,
    pub extends: BTreeSet<String>,
    pub conflicts: BTreeSet<String>,
    pub provides: BTreeSet<String>,
    pub requires: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BundleDef {
    pub bundle: String,
    pub includes: BTreeSet<String>,
    pub extends: BTreeSet<String>,
    pub constraints: BTreeMap<String, Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDef {
    pub bundle: String,
    pub projection: String,
    pub expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionDef {
    pub aspect: String,
    pub action: String,
    pub declared: bool,
    pub body: Option<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateSchema {
    pub aspect: String,
    pub state: String,
    pub type_expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegistrySchema {
    pub aspect: String,
    pub registry: String,
    pub type_expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvariantDef {
    pub aspect: String,
    pub invariant: String,
    pub expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MethodAbiDef {
    pub bundle: String,
    pub projection: String,
    pub args: Expr,
    pub return_type: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MethodPolicyIr {
    pub bundle: String,
    pub projection: String,
    pub authority: AuthorityKind,
    pub effects: BTreeSet<EffectKind>,
    pub invariants: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum AuthorityKind {
    TxSender,
    MsgSender,
    RoleGrant,
    Allowance,
    PermitCertificate,
    OracleUpdaterGrant,
    BridgeCertificate,
    GovernanceAdminGrant,
    Custom(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum EffectKind {
    ReadState,
    WriteState,
    ReadRegistry,
    WriteRegistry,
    ConsumeRegistryGrant,
    EmitEvent,
    CallContract,
    DeployContract,
    ScheduleUpgrade,
    ExecuteUpgrade,
    CrossShardOutboxAppend,
    UsePermitCertificate,
    Abort,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifiedAspectModule {
    pub ir: AspectModuleIr,
    pub bundle_aspect_closures: BTreeMap<String, BTreeSet<String>>,
    pub roots: AspectArtifactRoots,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectArtifactRoots {
    pub ir_root: String,
    pub abi_root: String,
    pub policy_root: String,
    pub storage_schema_root: String,
    pub registry_schema_root: String,
    pub invariant_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectModuleArtifact {
    pub schema: String,
    pub schema_version: u32,
    pub module_id: String,
    pub taxonomy_version: String,
    pub accepted_language: String,
    pub verifier_version: String,
    pub source_root: String,
    pub canonical_source: String,
    pub ir_root: String,
    pub abi_root: String,
    pub policy_root: String,
    pub storage_schema_root: String,
    pub registry_schema_root: String,
    pub invariant_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AspectError {
    UnexpectedEof,
    UnexpectedToken {
        expected: String,
        found: String,
    },
    EmptyList,
    UnknownDeclaration(String),
    InvalidArity {
        form: String,
        expected: usize,
        actual: usize,
    },
    ExpectedAtom {
        context: String,
        found: String,
    },
    ForbiddenForm(String),
    StringLiteralUnsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AspectVerifyError {
    Parse(String),
    DuplicateType(String),
    DuplicateAspect(String),
    DuplicateBundle(String),
    DuplicateProjection(String),
    DuplicateState(String),
    DuplicateRegistry(String),
    DuplicateAction(String),
    DuplicateInvariant(String),
    DuplicateAbi(String),
    DuplicatePolicy(String),
    UnknownAspectReference {
        context: String,
        aspect: String,
    },
    UnknownBundleReference {
        context: String,
        bundle: String,
    },
    UnknownTypeReference {
        context: String,
        type_name: String,
    },
    MissingActionDeclaration {
        aspect: String,
        action: String,
    },
    MissingDerivedBody {
        aspect: String,
        action: String,
    },
    DuplicateStateOwnership {
        state: String,
        first_aspect: String,
        second_aspect: String,
    },
    DuplicateRegistryOwnership {
        registry: String,
        first_aspect: String,
        second_aspect: String,
    },
    MissingRequiredFacet {
        bundle: String,
        aspect: String,
        facet: String,
    },
    ConflictingAspects {
        bundle: String,
        aspect: String,
        other: String,
    },
    ProjectionTargetUnsupported {
        bundle: String,
        projection: String,
    },
    MissingProjectionAbi {
        bundle: String,
        projection: String,
    },
    MissingProjectionPolicy {
        bundle: String,
        projection: String,
    },
    UnsupportedAuthority(String),
    UnsupportedEffect(String),
    MalformedEffectList(String),
    MalformedInvariantList(String),
    UndeclaredEffect {
        bundle: String,
        projection: String,
        effect: EffectKind,
    },
    StateWriteOutOfScope {
        bundle: String,
        projection: String,
        state: String,
    },
    RegistryAccessOutOfScope {
        bundle: String,
        projection: String,
        registry: String,
    },
    ReentrantActionCycle {
        action: String,
    },
    JsonSerialization(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    LParen,
    RParen,
    Atom(String),
}

pub fn parse_aspect_package(source: &str) -> Result<AspectPackageAst, AspectError> {
    let tokens = tokenize(source)?;
    let mut parser = SExprParser {
        tokens,
        position: 0,
    };
    let mut declarations = Vec::new();

    while !parser.is_eof() {
        let expr = parser.parse_expr()?;
        declarations.push(declaration_from_expr(expr)?);
    }

    Ok(AspectPackageAst { declarations })
}

pub fn canonical_aspect_source(ast: &AspectPackageAst) -> String {
    ast.declarations
        .iter()
        .map(canonical_declaration)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn canonical_source_root(ast: &AspectPackageAst) -> String {
    sha256_hex(canonical_aspect_source(ast).as_bytes())
}

pub fn parse_and_canonicalize(source: &str) -> Result<(AspectPackageAst, String), AspectError> {
    let ast = parse_aspect_package(source)?;
    let canonical = canonical_aspect_source(&ast);
    Ok((ast, canonical))
}

pub fn parse_and_source_root(source: &str) -> Result<String, AspectError> {
    let ast = parse_aspect_package(source)?;
    Ok(canonical_source_root(&ast))
}

pub fn lower_to_ir(ast: &AspectPackageAst) -> Result<AspectModuleIr, AspectVerifyError> {
    let mut ir = AspectModuleIr {
        taxonomy_version: TAXONOMY_VERSION_NORMALIZED_BALANCE_FIRST.into(),
        source_root: canonical_source_root(ast),
        types: BTreeMap::new(),
        aspects: BTreeMap::new(),
        bundles: BTreeMap::new(),
        projections: BTreeMap::new(),
        actions: BTreeMap::new(),
        storage_schema: BTreeMap::new(),
        registry_schema: BTreeMap::new(),
        invariants: BTreeMap::new(),
        abi: BTreeMap::new(),
        policies: BTreeMap::new(),
    };

    for declaration in &ast.declarations {
        match declaration {
            AspectDeclaration::Type { symbol, type_expr } => {
                insert_unique(
                    &mut ir.types,
                    symbol.clone(),
                    type_expr.clone(),
                    AspectVerifyError::DuplicateType(symbol.clone()),
                )?;
            }
            AspectDeclaration::Aspect { aspect } => {
                insert_aspect(&mut ir.aspects, aspect, false)?;
            }
            AspectDeclaration::AbstractAspect { aspect } => {
                insert_aspect(&mut ir.aspects, aspect, true)?;
            }
            AspectDeclaration::Bundle { bundle } => {
                insert_unique(
                    &mut ir.bundles,
                    bundle.clone(),
                    BundleDef {
                        bundle: bundle.clone(),
                        includes: BTreeSet::new(),
                        extends: BTreeSet::new(),
                        constraints: BTreeMap::new(),
                    },
                    AspectVerifyError::DuplicateBundle(bundle.clone()),
                )?;
            }
            AspectDeclaration::LayerOf { .. } => {}
            AspectDeclaration::Extends { aspect, parent } => {
                let def = aspect_def_mut(&mut ir.aspects, aspect)?;
                def.extends.insert(parent.clone());
            }
            AspectDeclaration::Conflicts { aspect, other } => {
                let def = aspect_def_mut(&mut ir.aspects, aspect)?;
                def.conflicts.insert(other.clone());
            }
            AspectDeclaration::Owns {
                aspect,
                state,
                type_expr,
            } => {
                let key = qualified(aspect, state);
                insert_unique(
                    &mut ir.storage_schema,
                    key,
                    StateSchema {
                        aspect: aspect.clone(),
                        state: state.clone(),
                        type_expr: type_expr.clone(),
                    },
                    AspectVerifyError::DuplicateState(state.clone()),
                )?;
            }
            AspectDeclaration::RegistryOwns {
                aspect,
                registry,
                type_expr,
            } => {
                let key = qualified(aspect, registry);
                insert_unique(
                    &mut ir.registry_schema,
                    key,
                    RegistrySchema {
                        aspect: aspect.clone(),
                        registry: registry.clone(),
                        type_expr: type_expr.clone(),
                    },
                    AspectVerifyError::DuplicateRegistry(registry.clone()),
                )?;
            }
            AspectDeclaration::Provides { aspect, facet } => {
                let def = aspect_def_mut(&mut ir.aspects, aspect)?;
                def.provides.insert(facet.clone());
            }
            AspectDeclaration::Requires { aspect, facet } => {
                let def = aspect_def_mut(&mut ir.aspects, aspect)?;
                def.requires.insert(facet.clone());
            }
            AspectDeclaration::Action { aspect, action } => {
                let key = qualified(aspect, action);
                match ir.actions.get_mut(&key) {
                    Some(existing) if existing.declared => {
                        return Err(AspectVerifyError::DuplicateAction(key));
                    }
                    Some(existing) => existing.declared = true,
                    None => {
                        ir.actions.insert(
                            key,
                            ActionDef {
                                aspect: aspect.clone(),
                                action: action.clone(),
                                declared: true,
                                body: None,
                            },
                        );
                    }
                }
            }
            AspectDeclaration::Derived {
                aspect,
                action,
                expr,
            } => {
                let key = qualified(aspect, action);
                match ir.actions.get_mut(&key) {
                    Some(existing) if existing.body.is_some() => {
                        return Err(AspectVerifyError::DuplicateAction(key));
                    }
                    Some(existing) => existing.body = Some(expr.clone()),
                    None => {
                        ir.actions.insert(
                            key,
                            ActionDef {
                                aspect: aspect.clone(),
                                action: action.clone(),
                                declared: false,
                                body: Some(expr.clone()),
                            },
                        );
                    }
                }
            }
            AspectDeclaration::LocalInvariant {
                aspect,
                invariant,
                expr,
            } => {
                let key = qualified(aspect, invariant);
                insert_unique(
                    &mut ir.invariants,
                    key,
                    InvariantDef {
                        aspect: aspect.clone(),
                        invariant: invariant.clone(),
                        expr: expr.clone(),
                    },
                    AspectVerifyError::DuplicateInvariant(qualified(aspect, invariant)),
                )?;
            }
            AspectDeclaration::BundleIncludes { bundle, aspect } => {
                let def = bundle_def_mut(&mut ir.bundles, bundle)?;
                def.includes.insert(aspect.clone());
            }
            AspectDeclaration::BundleExtends { bundle, parent } => {
                let def = bundle_def_mut(&mut ir.bundles, bundle)?;
                def.extends.insert(parent.clone());
            }
            AspectDeclaration::BundleConstraint {
                bundle,
                constraint,
                expr,
            } => {
                let def = bundle_def_mut(&mut ir.bundles, bundle)?;
                if def
                    .constraints
                    .insert(constraint.clone(), expr.clone())
                    .is_some()
                {
                    return Err(AspectVerifyError::DuplicateInvariant(qualified(
                        bundle, constraint,
                    )));
                }
            }
            AspectDeclaration::Projection {
                bundle,
                projection,
                expr,
            } => {
                let key = qualified(bundle, projection);
                insert_unique(
                    &mut ir.projections,
                    key.clone(),
                    ProjectionDef {
                        bundle: bundle.clone(),
                        projection: projection.clone(),
                        expr: expr.clone(),
                    },
                    AspectVerifyError::DuplicateProjection(key),
                )?;
            }
            AspectDeclaration::MethodAbi {
                bundle,
                projection,
                args,
                return_type,
            } => {
                let key = qualified(bundle, projection);
                insert_unique(
                    &mut ir.abi,
                    key.clone(),
                    MethodAbiDef {
                        bundle: bundle.clone(),
                        projection: projection.clone(),
                        args: args.clone(),
                        return_type: return_type.clone(),
                    },
                    AspectVerifyError::DuplicateAbi(key),
                )?;
            }
            AspectDeclaration::MethodPolicy {
                bundle,
                projection,
                authority,
                effects,
                invariants,
            } => {
                let key = qualified(bundle, projection);
                insert_unique(
                    &mut ir.policies,
                    key.clone(),
                    MethodPolicyIr {
                        bundle: bundle.clone(),
                        projection: projection.clone(),
                        authority: parse_authority(authority)?,
                        effects: parse_effects(effects)?,
                        invariants: parse_invariants(invariants)?,
                    },
                    AspectVerifyError::DuplicatePolicy(key),
                )?;
            }
            AspectDeclaration::CrossConstraint { .. } => {}
        }
    }

    Ok(ir)
}

pub fn verify_module(ir: &AspectModuleIr) -> Result<VerifiedAspectModule, AspectVerifyError> {
    verify_references(ir)?;
    verify_type_expressions(ir)?;
    verify_action_declarations(ir)?;
    verify_state_ownership(ir)?;
    verify_registry_ownership(ir)?;
    verify_action_call_graph(ir)?;
    let bundle_aspect_closures = bundle_aspect_closures(ir)?;
    verify_required_facets(ir, &bundle_aspect_closures)?;
    verify_conflicts(ir, &bundle_aspect_closures)?;
    verify_projections(ir, &bundle_aspect_closures)?;
    verify_projection_abi_and_policy(ir)?;
    verify_effects_and_write_scopes(ir, &bundle_aspect_closures)?;

    Ok(VerifiedAspectModule {
        ir: ir.clone(),
        bundle_aspect_closures,
        roots: artifact_roots(ir)?,
    })
}

pub fn module_artifact(
    module_id: &str,
    canonical_source: String,
    module: &VerifiedAspectModule,
) -> AspectModuleArtifact {
    AspectModuleArtifact {
        schema: ASPECT_SOURCE_SCHEMA.into(),
        schema_version: ASPECT_SOURCE_SCHEMA_VERSION,
        module_id: module_id.into(),
        taxonomy_version: module.ir.taxonomy_version.clone(),
        accepted_language: ASPECT_LANGUAGE.into(),
        verifier_version: "detta-aspects-0".into(),
        source_root: module.ir.source_root.clone(),
        canonical_source,
        ir_root: module.roots.ir_root.clone(),
        abi_root: module.roots.abi_root.clone(),
        policy_root: module.roots.policy_root.clone(),
        storage_schema_root: module.roots.storage_schema_root.clone(),
        registry_schema_root: module.roots.registry_schema_root.clone(),
        invariant_root: module.roots.invariant_root.clone(),
    }
}

pub fn parse_verify_module(
    source: &str,
) -> Result<(String, AspectModuleIr, VerifiedAspectModule), AspectVerifyError> {
    let ast = parse_aspect_package(source)
        .map_err(|error| AspectVerifyError::Parse(format!("{error:?}")))?;
    let canonical = canonical_aspect_source(&ast);
    let ir = lower_to_ir(&ast)?;
    let verified = verify_module(&ir)?;
    Ok((canonical, ir, verified))
}

fn tokenize(source: &str) -> Result<Vec<Token>, AspectError> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            ';' => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '"' => return Err(AspectError::StringLiteralUnsupported),
            ch if ch.is_whitespace() => {}
            ch => {
                let mut atom = String::from(ch);
                while let Some(next) = chars.peek().copied() {
                    if next.is_whitespace() || next == '(' || next == ')' || next == ';' {
                        break;
                    }
                    if next == '"' {
                        return Err(AspectError::StringLiteralUnsupported);
                    }
                    atom.push(next);
                    chars.next();
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }

    Ok(tokens)
}

struct SExprParser {
    tokens: Vec<Token>,
    position: usize,
}

impl SExprParser {
    fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn parse_expr(&mut self) -> Result<Expr, AspectError> {
        match self.next() {
            Some(Token::Atom(atom)) => Ok(Expr::Atom(atom)),
            Some(Token::LParen) => {
                let mut items = Vec::new();
                loop {
                    match self.peek() {
                        Some(Token::RParen) => {
                            self.next();
                            return Ok(Expr::List(items));
                        }
                        Some(_) => items.push(self.parse_expr()?),
                        None => return Err(AspectError::UnexpectedEof),
                    }
                }
            }
            Some(Token::RParen) => Err(AspectError::UnexpectedToken {
                expected: "expression".into(),
                found: ")".into(),
            }),
            None => Err(AspectError::UnexpectedEof),
        }
    }
}

fn declaration_from_expr(expr: Expr) -> Result<AspectDeclaration, AspectError> {
    reject_forbidden_forms(&expr)?;
    let items = match expr {
        Expr::List(items) if items.is_empty() => return Err(AspectError::EmptyList),
        Expr::List(items) => items,
        Expr::Atom(atom) => return Err(AspectError::UnknownDeclaration(atom)),
    };
    let head = atom_at(&items, 0, "declaration head")?.to_owned();

    match head.as_str() {
        ":" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Type {
                symbol: atom_at(&items, 1, "type symbol")?.to_owned(),
                type_expr: items[2].clone(),
            })
        }
        "aspect" => {
            expect_arity(&head, &items, 2)?;
            Ok(AspectDeclaration::Aspect {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
            })
        }
        "abstract-aspect" => {
            expect_arity(&head, &items, 2)?;
            Ok(AspectDeclaration::AbstractAspect {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
            })
        }
        "bundle" => {
            expect_arity(&head, &items, 2)?;
            Ok(AspectDeclaration::Bundle {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
            })
        }
        "layer-of" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::LayerOf {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                layer: atom_at(&items, 2, "layer id")?.to_owned(),
            })
        }
        "extends" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Extends {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                parent: atom_at(&items, 2, "parent aspect id")?.to_owned(),
            })
        }
        "conflicts" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Conflicts {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                other: atom_at(&items, 2, "conflicting aspect id")?.to_owned(),
            })
        }
        "owns" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::Owns {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                state: atom_at(&items, 2, "state id")?.to_owned(),
                type_expr: items[3].clone(),
            })
        }
        "registry-owns" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::RegistryOwns {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                registry: atom_at(&items, 2, "registry id")?.to_owned(),
                type_expr: items[3].clone(),
            })
        }
        "provides" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Provides {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                facet: atom_at(&items, 2, "facet id")?.to_owned(),
            })
        }
        "requires" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Requires {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                facet: atom_at(&items, 2, "facet id")?.to_owned(),
            })
        }
        "action" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::Action {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                action: atom_at(&items, 2, "action id")?.to_owned(),
            })
        }
        "derived" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::Derived {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                action: atom_at(&items, 2, "action id")?.to_owned(),
                expr: items[3].clone(),
            })
        }
        "local-invariant" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::LocalInvariant {
                aspect: atom_at(&items, 1, "aspect id")?.to_owned(),
                invariant: atom_at(&items, 2, "invariant id")?.to_owned(),
                expr: items[3].clone(),
            })
        }
        "bundle-includes" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::BundleIncludes {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                aspect: atom_at(&items, 2, "aspect id")?.to_owned(),
            })
        }
        "bundle-extends" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::BundleExtends {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                parent: atom_at(&items, 2, "parent bundle id")?.to_owned(),
            })
        }
        "bundle-constraint" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::BundleConstraint {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                constraint: atom_at(&items, 2, "constraint id")?.to_owned(),
                expr: items[3].clone(),
            })
        }
        "projection" => {
            expect_arity(&head, &items, 4)?;
            Ok(AspectDeclaration::Projection {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                projection: atom_at(&items, 2, "projection id")?.to_owned(),
                expr: items[3].clone(),
            })
        }
        "method-abi" => {
            expect_arity(&head, &items, 5)?;
            Ok(AspectDeclaration::MethodAbi {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                projection: atom_at(&items, 2, "projection id")?.to_owned(),
                args: items[3].clone(),
                return_type: items[4].clone(),
            })
        }
        "method-policy" => {
            expect_arity(&head, &items, 6)?;
            Ok(AspectDeclaration::MethodPolicy {
                bundle: atom_at(&items, 1, "bundle id")?.to_owned(),
                projection: atom_at(&items, 2, "projection id")?.to_owned(),
                authority: items[3].clone(),
                effects: items[4].clone(),
                invariants: items[5].clone(),
            })
        }
        "cross-constraint" => {
            expect_arity(&head, &items, 3)?;
            Ok(AspectDeclaration::CrossConstraint {
                constraint: atom_at(&items, 1, "constraint id")?.to_owned(),
                expr: items[2].clone(),
            })
        }
        _ => Err(AspectError::UnknownDeclaration(head)),
    }
}

fn atom_at<'a>(items: &'a [Expr], index: usize, context: &str) -> Result<&'a str, AspectError> {
    match items.get(index) {
        Some(Expr::Atom(atom)) => Ok(atom),
        Some(expr) => Err(AspectError::ExpectedAtom {
            context: context.into(),
            found: canonical_expr(expr),
        }),
        None => Err(AspectError::UnexpectedEof),
    }
}

fn expect_arity(form: &str, items: &[Expr], expected: usize) -> Result<(), AspectError> {
    if items.len() == expected {
        return Ok(());
    }
    Err(AspectError::InvalidArity {
        form: form.into(),
        expected: expected - 1,
        actual: items.len().saturating_sub(1),
    })
}

fn reject_forbidden_forms(expr: &Expr) -> Result<(), AspectError> {
    match expr {
        Expr::Atom(_) => Ok(()),
        Expr::List(items) => {
            if let Some(Expr::Atom(head)) = items.first() {
                if FORBIDDEN_FORMS.contains(&head.as_str()) {
                    return Err(AspectError::ForbiddenForm(head.clone()));
                }
            }
            for item in items {
                reject_forbidden_forms(item)?;
            }
            Ok(())
        }
    }
}

fn canonical_declaration(declaration: &AspectDeclaration) -> String {
    match declaration {
        AspectDeclaration::Type { symbol, type_expr } => {
            format!("(: {symbol} {})", canonical_expr(type_expr))
        }
        AspectDeclaration::Aspect { aspect } => format!("(aspect {aspect})"),
        AspectDeclaration::AbstractAspect { aspect } => format!("(abstract-aspect {aspect})"),
        AspectDeclaration::Bundle { bundle } => format!("(bundle {bundle})"),
        AspectDeclaration::LayerOf { aspect, layer } => format!("(layer-of {aspect} {layer})"),
        AspectDeclaration::Extends { aspect, parent } => format!("(extends {aspect} {parent})"),
        AspectDeclaration::Conflicts { aspect, other } => format!("(conflicts {aspect} {other})"),
        AspectDeclaration::Owns {
            aspect,
            state,
            type_expr,
        } => format!("(owns {aspect} {state} {})", canonical_expr(type_expr)),
        AspectDeclaration::RegistryOwns {
            aspect,
            registry,
            type_expr,
        } => format!(
            "(registry-owns {aspect} {registry} {})",
            canonical_expr(type_expr)
        ),
        AspectDeclaration::Provides { aspect, facet } => format!("(provides {aspect} {facet})"),
        AspectDeclaration::Requires { aspect, facet } => format!("(requires {aspect} {facet})"),
        AspectDeclaration::Action { aspect, action } => format!("(action {aspect} {action})"),
        AspectDeclaration::Derived {
            aspect,
            action,
            expr,
        } => format!("(derived {aspect} {action} {})", canonical_expr(expr)),
        AspectDeclaration::LocalInvariant {
            aspect,
            invariant,
            expr,
        } => format!(
            "(local-invariant {aspect} {invariant} {})",
            canonical_expr(expr)
        ),
        AspectDeclaration::BundleIncludes { bundle, aspect } => {
            format!("(bundle-includes {bundle} {aspect})")
        }
        AspectDeclaration::BundleExtends { bundle, parent } => {
            format!("(bundle-extends {bundle} {parent})")
        }
        AspectDeclaration::BundleConstraint {
            bundle,
            constraint,
            expr,
        } => format!(
            "(bundle-constraint {bundle} {constraint} {})",
            canonical_expr(expr)
        ),
        AspectDeclaration::Projection {
            bundle,
            projection,
            expr,
        } => format!(
            "(projection {bundle} {projection} {})",
            canonical_expr(expr)
        ),
        AspectDeclaration::MethodAbi {
            bundle,
            projection,
            args,
            return_type,
        } => format!(
            "(method-abi {bundle} {projection} {} {})",
            canonical_expr(args),
            canonical_expr(return_type)
        ),
        AspectDeclaration::MethodPolicy {
            bundle,
            projection,
            authority,
            effects,
            invariants,
        } => format!(
            "(method-policy {bundle} {projection} {} {} {})",
            canonical_expr(authority),
            canonical_expr(effects),
            canonical_expr(invariants)
        ),
        AspectDeclaration::CrossConstraint { constraint, expr } => {
            format!("(cross-constraint {constraint} {})", canonical_expr(expr))
        }
    }
}

fn canonical_expr(expr: &Expr) -> String {
    match expr {
        Expr::Atom(atom) => atom.clone(),
        Expr::List(items) => {
            let body = items
                .iter()
                .map(canonical_expr)
                .collect::<Vec<_>>()
                .join(" ");
            format!("({body})")
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn insert_unique<K: Ord, V>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    error: AspectVerifyError,
) -> Result<(), AspectVerifyError> {
    if map.contains_key(&key) {
        return Err(error);
    }
    map.insert(key, value);
    Ok(())
}

fn insert_aspect(
    aspects: &mut BTreeMap<String, AspectDef>,
    aspect: &str,
    is_abstract: bool,
) -> Result<(), AspectVerifyError> {
    if aspects.contains_key(aspect) {
        return Err(AspectVerifyError::DuplicateAspect(aspect.into()));
    }
    aspects.insert(
        aspect.into(),
        AspectDef {
            aspect: aspect.into(),
            is_abstract,
            extends: BTreeSet::new(),
            conflicts: BTreeSet::new(),
            provides: BTreeSet::new(),
            requires: BTreeSet::new(),
        },
    );
    Ok(())
}

fn aspect_def_mut<'a>(
    aspects: &'a mut BTreeMap<String, AspectDef>,
    aspect: &str,
) -> Result<&'a mut AspectDef, AspectVerifyError> {
    aspects
        .get_mut(aspect)
        .ok_or_else(|| AspectVerifyError::UnknownAspectReference {
            context: "aspect declaration".into(),
            aspect: aspect.into(),
        })
}

fn bundle_def_mut<'a>(
    bundles: &'a mut BTreeMap<String, BundleDef>,
    bundle: &str,
) -> Result<&'a mut BundleDef, AspectVerifyError> {
    bundles
        .get_mut(bundle)
        .ok_or_else(|| AspectVerifyError::UnknownBundleReference {
            context: "bundle declaration".into(),
            bundle: bundle.into(),
        })
}

fn qualified(namespace: &str, name: &str) -> String {
    format!("{namespace}::{name}")
}

fn verify_references(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    for aspect in ir.aspects.values() {
        for parent in &aspect.extends {
            if !ir.aspects.contains_key(parent) {
                return Err(AspectVerifyError::UnknownAspectReference {
                    context: format!("{} extends", aspect.aspect),
                    aspect: parent.clone(),
                });
            }
        }
        for other in &aspect.conflicts {
            if !ir.aspects.contains_key(other) {
                return Err(AspectVerifyError::UnknownAspectReference {
                    context: format!("{} conflicts", aspect.aspect),
                    aspect: other.clone(),
                });
            }
        }
    }

    for schema in ir.storage_schema.values() {
        if !ir.aspects.contains_key(&schema.aspect) {
            return Err(AspectVerifyError::UnknownAspectReference {
                context: format!("{} owns", schema.state),
                aspect: schema.aspect.clone(),
            });
        }
    }

    for schema in ir.registry_schema.values() {
        if !ir.aspects.contains_key(&schema.aspect) {
            return Err(AspectVerifyError::UnknownAspectReference {
                context: format!("{} registry-owns", schema.registry),
                aspect: schema.aspect.clone(),
            });
        }
    }

    for action in ir.actions.values() {
        if !ir.aspects.contains_key(&action.aspect) {
            return Err(AspectVerifyError::UnknownAspectReference {
                context: format!("{} action", action.action),
                aspect: action.aspect.clone(),
            });
        }
    }

    for invariant in ir.invariants.values() {
        if !ir.aspects.contains_key(&invariant.aspect) {
            return Err(AspectVerifyError::UnknownAspectReference {
                context: format!("{} invariant", invariant.invariant),
                aspect: invariant.aspect.clone(),
            });
        }
    }

    for bundle in ir.bundles.values() {
        for aspect in &bundle.includes {
            if !ir.aspects.contains_key(aspect) {
                return Err(AspectVerifyError::UnknownAspectReference {
                    context: format!("{} includes", bundle.bundle),
                    aspect: aspect.clone(),
                });
            }
        }
        for parent in &bundle.extends {
            if !ir.bundles.contains_key(parent) {
                return Err(AspectVerifyError::UnknownBundleReference {
                    context: format!("{} extends", bundle.bundle),
                    bundle: parent.clone(),
                });
            }
        }
    }

    for projection in ir.projections.values() {
        if !ir.bundles.contains_key(&projection.bundle) {
            return Err(AspectVerifyError::UnknownBundleReference {
                context: format!("{} projection", projection.projection),
                bundle: projection.bundle.clone(),
            });
        }
    }

    for abi in ir.abi.values() {
        let key = qualified(&abi.bundle, &abi.projection);
        if !ir.projections.contains_key(&key) {
            return Err(AspectVerifyError::MissingProjectionAbi {
                bundle: abi.bundle.clone(),
                projection: abi.projection.clone(),
            });
        }
    }

    for policy in ir.policies.values() {
        let key = qualified(&policy.bundle, &policy.projection);
        if !ir.projections.contains_key(&key) {
            return Err(AspectVerifyError::MissingProjectionPolicy {
                bundle: policy.bundle.clone(),
                projection: policy.projection.clone(),
            });
        }
    }

    Ok(())
}

fn verify_type_expressions(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    let mut known_types = BTreeSet::from([
        "->".to_string(),
        "Type".to_string(),
        "Bool".to_string(),
        "UInt8".to_string(),
        "UInt64".to_string(),
        "Amount".to_string(),
        "Address".to_string(),
        "Asset".to_string(),
        "Contract".to_string(),
        "Method".to_string(),
        "Role".to_string(),
        "Bytes".to_string(),
        "String".to_string(),
        "Time".to_string(),
        "Height".to_string(),
        "Event".to_string(),
    ]);
    known_types.extend(ir.types.keys().cloned());

    for (symbol, type_expr) in &ir.types {
        verify_type_expr(
            type_expr,
            &known_types,
            &format!("type declaration {symbol}"),
        )?;
    }
    for schema in ir.storage_schema.values() {
        verify_type_expr(
            &schema.type_expr,
            &known_types,
            &format!("state {}::{}", schema.aspect, schema.state),
        )?;
    }
    for schema in ir.registry_schema.values() {
        verify_type_expr(
            &schema.type_expr,
            &known_types,
            &format!("registry {}::{}", schema.aspect, schema.registry),
        )?;
    }
    for abi in ir.abi.values() {
        verify_abi_args(abi, &known_types)?;
        verify_type_expr(
            &abi.return_type,
            &known_types,
            &format!("return type {}::{}", abi.bundle, abi.projection),
        )?;
    }
    Ok(())
}

fn verify_type_expr(
    expr: &Expr,
    known_types: &BTreeSet<String>,
    context: &str,
) -> Result<(), AspectVerifyError> {
    match expr {
        Expr::Atom(atom) => {
            if known_types.contains(atom) {
                Ok(())
            } else {
                Err(AspectVerifyError::UnknownTypeReference {
                    context: context.into(),
                    type_name: atom.clone(),
                })
            }
        }
        Expr::List(items) => {
            for item in items {
                verify_type_expr(item, known_types, context)?;
            }
            Ok(())
        }
    }
}

fn verify_abi_args(
    abi: &MethodAbiDef,
    known_types: &BTreeSet<String>,
) -> Result<(), AspectVerifyError> {
    let items = match &abi.args {
        Expr::List(items) => items,
        Expr::Atom(atom) => {
            return Err(AspectVerifyError::UnknownTypeReference {
                context: format!("abi args {}::{}", abi.bundle, abi.projection),
                type_name: atom.clone(),
            });
        }
    };
    match items.first() {
        Some(Expr::Atom(head)) if head == "args" => {}
        Some(expr) => {
            return Err(AspectVerifyError::UnknownTypeReference {
                context: format!("abi args {}::{}", abi.bundle, abi.projection),
                type_name: canonical_expr(expr),
            });
        }
        None => return Ok(()),
    }
    for arg in items.iter().skip(1) {
        match arg {
            Expr::List(pair) if pair.len() == 2 => {
                match &pair[0] {
                    Expr::Atom(_) => {}
                    expr => {
                        return Err(AspectVerifyError::UnknownTypeReference {
                            context: format!("abi arg name {}::{}", abi.bundle, abi.projection),
                            type_name: canonical_expr(expr),
                        });
                    }
                }
                verify_type_expr(
                    &pair[1],
                    known_types,
                    &format!("abi arg {}::{}", abi.bundle, abi.projection),
                )?;
            }
            expr => {
                return Err(AspectVerifyError::UnknownTypeReference {
                    context: format!("abi args {}::{}", abi.bundle, abi.projection),
                    type_name: canonical_expr(expr),
                });
            }
        }
    }
    Ok(())
}

fn verify_action_declarations(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    for action in ir.actions.values() {
        if !action.declared {
            return Err(AspectVerifyError::MissingActionDeclaration {
                aspect: action.aspect.clone(),
                action: action.action.clone(),
            });
        }
    }
    Ok(())
}

fn verify_state_ownership(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for schema in ir.storage_schema.values() {
        if let Some(first_aspect) = owners.insert(&schema.state, &schema.aspect) {
            return Err(AspectVerifyError::DuplicateStateOwnership {
                state: schema.state.clone(),
                first_aspect: first_aspect.into(),
                second_aspect: schema.aspect.clone(),
            });
        }
    }
    Ok(())
}

fn verify_registry_ownership(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for schema in ir.registry_schema.values() {
        if let Some(first_aspect) = owners.insert(&schema.registry, &schema.aspect) {
            return Err(AspectVerifyError::DuplicateRegistryOwnership {
                registry: schema.registry.clone(),
                first_aspect: first_aspect.into(),
                second_aspect: schema.aspect.clone(),
            });
        }
    }
    Ok(())
}

fn bundle_aspect_closures(
    ir: &AspectModuleIr,
) -> Result<BTreeMap<String, BTreeSet<String>>, AspectVerifyError> {
    let mut closures = BTreeMap::new();
    for bundle in ir.bundles.keys() {
        let mut closure = BTreeSet::new();
        let mut visiting_bundles = BTreeSet::new();
        collect_bundle_aspects(ir, bundle, &mut visiting_bundles, &mut closure)?;
        closures.insert(bundle.clone(), closure);
    }
    Ok(closures)
}

fn collect_bundle_aspects(
    ir: &AspectModuleIr,
    bundle: &str,
    visiting_bundles: &mut BTreeSet<String>,
    closure: &mut BTreeSet<String>,
) -> Result<(), AspectVerifyError> {
    if !visiting_bundles.insert(bundle.into()) {
        return Ok(());
    }
    let def = ir
        .bundles
        .get(bundle)
        .ok_or_else(|| AspectVerifyError::UnknownBundleReference {
            context: "bundle closure".into(),
            bundle: bundle.into(),
        })?;

    for parent in &def.extends {
        collect_bundle_aspects(ir, parent, visiting_bundles, closure)?;
    }
    for aspect in &def.includes {
        let mut visiting_aspects = BTreeSet::new();
        collect_aspect_with_parents(ir, aspect, &mut visiting_aspects, closure)?;
    }
    visiting_bundles.remove(bundle);
    Ok(())
}

fn collect_aspect_with_parents(
    ir: &AspectModuleIr,
    aspect: &str,
    visiting_aspects: &mut BTreeSet<String>,
    closure: &mut BTreeSet<String>,
) -> Result<(), AspectVerifyError> {
    if !visiting_aspects.insert(aspect.into()) {
        return Ok(());
    }
    let def = ir
        .aspects
        .get(aspect)
        .ok_or_else(|| AspectVerifyError::UnknownAspectReference {
            context: "aspect closure".into(),
            aspect: aspect.into(),
        })?;

    for parent in &def.extends {
        collect_aspect_with_parents(ir, parent, visiting_aspects, closure)?;
    }
    closure.insert(aspect.into());
    visiting_aspects.remove(aspect);
    Ok(())
}

fn verify_required_facets(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), AspectVerifyError> {
    for (bundle, closure) in closures {
        let mut provided = BTreeSet::new();
        for aspect in closure {
            if let Some(def) = ir.aspects.get(aspect) {
                provided.extend(def.provides.iter().cloned());
            }
        }
        for aspect in closure {
            let Some(def) = ir.aspects.get(aspect) else {
                continue;
            };
            for facet in &def.requires {
                if !provided.contains(facet) {
                    return Err(AspectVerifyError::MissingRequiredFacet {
                        bundle: bundle.clone(),
                        aspect: aspect.clone(),
                        facet: facet.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn verify_conflicts(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), AspectVerifyError> {
    for (bundle, closure) in closures {
        for aspect in closure {
            let Some(def) = ir.aspects.get(aspect) else {
                continue;
            };
            for other in &def.conflicts {
                if closure.contains(other) {
                    return Err(AspectVerifyError::ConflictingAspects {
                        bundle: bundle.clone(),
                        aspect: aspect.clone(),
                        other: other.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn verify_projections(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), AspectVerifyError> {
    for projection in ir.projections.values() {
        let Some(target) = projection_target_action(&projection.expr) else {
            return Err(AspectVerifyError::ProjectionTargetUnsupported {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            });
        };
        let Some(closure) = closures.get(&projection.bundle) else {
            return Err(AspectVerifyError::UnknownBundleReference {
                context: format!("{} projection", projection.projection),
                bundle: projection.bundle.clone(),
            });
        };
        let target_found = ir
            .actions
            .values()
            .any(|action| action.action == target && closure.contains(&action.aspect));
        if !target_found {
            return Err(AspectVerifyError::ProjectionTargetUnsupported {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            });
        }
    }
    Ok(())
}

fn projection_target_action(expr: &Expr) -> Option<&str> {
    let Expr::List(items) = expr else {
        return None;
    };
    if items.len() != 3 {
        return None;
    }
    match items.first() {
        Some(Expr::Atom(head)) if head == "=" => {}
        _ => return None,
    }
    let Expr::List(rhs) = &items[2] else {
        return None;
    };
    match rhs.first() {
        Some(Expr::Atom(action)) => Some(action.as_str()),
        _ => None,
    }
}

fn verify_projection_abi_and_policy(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    for projection in ir.projections.values() {
        let key = qualified(&projection.bundle, &projection.projection);
        if !ir.abi.contains_key(&key) {
            return Err(AspectVerifyError::MissingProjectionAbi {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            });
        }
        if !ir.policies.contains_key(&key) {
            return Err(AspectVerifyError::MissingProjectionPolicy {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            });
        }
    }
    Ok(())
}

fn artifact_roots(ir: &AspectModuleIr) -> Result<AspectArtifactRoots, AspectVerifyError> {
    Ok(AspectArtifactRoots {
        ir_root: sha256_json(ir)?,
        abi_root: sha256_json(&ir.abi)?,
        policy_root: sha256_json(&ir.policies)?,
        storage_schema_root: sha256_json(&ir.storage_schema)?,
        registry_schema_root: sha256_json(&ir.registry_schema)?,
        invariant_root: sha256_json(&ir.invariants)?,
    })
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String, AspectVerifyError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| AspectVerifyError::JsonSerialization(error.to_string()))?;
    Ok(sha256_hex(&bytes))
}

fn parse_authority(expr: &Expr) -> Result<AuthorityKind, AspectVerifyError> {
    let Expr::Atom(authority) = expr else {
        return Err(AspectVerifyError::UnsupportedAuthority(canonical_expr(
            expr,
        )));
    };
    Ok(match authority.as_str() {
        "TxSender" => AuthorityKind::TxSender,
        "MsgSender" => AuthorityKind::MsgSender,
        "RoleGrant" => AuthorityKind::RoleGrant,
        "Allowance" => AuthorityKind::Allowance,
        "PermitCertificate" => AuthorityKind::PermitCertificate,
        "OracleUpdaterGrant" => AuthorityKind::OracleUpdaterGrant,
        "BridgeCertificate" => AuthorityKind::BridgeCertificate,
        "GovernanceAdminGrant" => AuthorityKind::GovernanceAdminGrant,
        authority if authority.starts_with("Custom") => AuthorityKind::Custom(authority.into()),
        _ => return Err(AspectVerifyError::UnsupportedAuthority(authority.clone())),
    })
}

fn parse_effects(expr: &Expr) -> Result<BTreeSet<EffectKind>, AspectVerifyError> {
    let Expr::List(items) = expr else {
        return Err(AspectVerifyError::MalformedEffectList(canonical_expr(expr)));
    };
    match items.first() {
        Some(Expr::Atom(head)) if head == "effects" => {}
        _ => return Err(AspectVerifyError::MalformedEffectList(canonical_expr(expr))),
    }
    let mut effects = BTreeSet::new();
    for item in items.iter().skip(1) {
        let Expr::Atom(effect) = item else {
            return Err(AspectVerifyError::MalformedEffectList(canonical_expr(expr)));
        };
        effects.insert(parse_effect_kind(effect)?);
    }
    Ok(effects)
}

fn parse_effect_kind(effect: &str) -> Result<EffectKind, AspectVerifyError> {
    Ok(match effect {
        "ReadState" => EffectKind::ReadState,
        "WriteState" => EffectKind::WriteState,
        "ReadRegistry" => EffectKind::ReadRegistry,
        "WriteRegistry" => EffectKind::WriteRegistry,
        "ConsumeRegistryGrant" => EffectKind::ConsumeRegistryGrant,
        "EmitEvent" => EffectKind::EmitEvent,
        "CallContract" => EffectKind::CallContract,
        "DeployContract" => EffectKind::DeployContract,
        "ScheduleUpgrade" => EffectKind::ScheduleUpgrade,
        "ExecuteUpgrade" => EffectKind::ExecuteUpgrade,
        "CrossShardOutboxAppend" => EffectKind::CrossShardOutboxAppend,
        "UsePermitCertificate" => EffectKind::UsePermitCertificate,
        "Abort" => EffectKind::Abort,
        _ => return Err(AspectVerifyError::UnsupportedEffect(effect.into())),
    })
}

fn parse_invariants(expr: &Expr) -> Result<BTreeSet<String>, AspectVerifyError> {
    let Expr::List(items) = expr else {
        return Err(AspectVerifyError::MalformedInvariantList(canonical_expr(
            expr,
        )));
    };
    match items.first() {
        Some(Expr::Atom(head)) if head == "invariants" => {}
        _ => {
            return Err(AspectVerifyError::MalformedInvariantList(canonical_expr(
                expr,
            )))
        }
    }
    let mut invariants = BTreeSet::new();
    for item in items.iter().skip(1) {
        let Expr::Atom(invariant) = item else {
            return Err(AspectVerifyError::MalformedInvariantList(canonical_expr(
                expr,
            )));
        };
        invariants.insert(invariant.clone());
    }
    Ok(invariants)
}

fn verify_action_call_graph(ir: &AspectModuleIr) -> Result<(), AspectVerifyError> {
    let action_names = action_names(ir);
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for action in ir.actions.values() {
        let key = qualified(&action.aspect, &action.action);
        let mut calls = BTreeSet::new();
        if let Some(body) = &action.body {
            collect_action_calls(body, &action_names, &mut calls);
        }
        graph.insert(key, calls);
    }
    for action in graph.keys() {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        detect_action_cycle(action, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn detect_action_cycle(
    action: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), AspectVerifyError> {
    if visited.contains(action) {
        return Ok(());
    }
    if !visiting.insert(action.into()) {
        return Err(AspectVerifyError::ReentrantActionCycle {
            action: action.into(),
        });
    }
    if let Some(callees) = graph.get(action) {
        for callee in callees {
            detect_action_cycle(callee, graph, visiting, visited)?;
        }
    }
    visiting.remove(action);
    visited.insert(action.into());
    Ok(())
}

fn verify_effects_and_write_scopes(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), AspectVerifyError> {
    let action_names = action_names(ir);
    for projection in ir.projections.values() {
        let key = qualified(&projection.bundle, &projection.projection);
        let policy =
            ir.policies
                .get(&key)
                .ok_or_else(|| AspectVerifyError::MissingProjectionPolicy {
                    bundle: projection.bundle.clone(),
                    projection: projection.projection.clone(),
                })?;
        let Some(target) = projection_target_action(&projection.expr) else {
            return Err(AspectVerifyError::ProjectionTargetUnsupported {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            });
        };
        let action = action_for_projection_target(ir, closures, &projection.bundle, target)
            .ok_or_else(|| AspectVerifyError::ProjectionTargetUnsupported {
                bundle: projection.bundle.clone(),
                projection: projection.projection.clone(),
            })?;
        let mut inferred = BTreeSet::new();
        let mut writes = BTreeSet::new();
        let mut registry_accesses = BTreeSet::new();
        let mut visited = BTreeSet::new();
        infer_action_effects(
            ir,
            &action_names,
            &qualified(&action.aspect, &action.action),
            &mut visited,
            &mut inferred,
            &mut writes,
            &mut registry_accesses,
        );

        for effect in inferred {
            if !policy.effects.contains(&effect) {
                return Err(AspectVerifyError::UndeclaredEffect {
                    bundle: projection.bundle.clone(),
                    projection: projection.projection.clone(),
                    effect,
                });
            }
        }

        for state in writes {
            if !state_owned_by_bundle(ir, closures, &projection.bundle, &state) {
                return Err(AspectVerifyError::StateWriteOutOfScope {
                    bundle: projection.bundle.clone(),
                    projection: projection.projection.clone(),
                    state,
                });
            }
        }

        for registry in registry_accesses {
            if !registry_owned_by_bundle(ir, closures, &projection.bundle, &registry) {
                return Err(AspectVerifyError::RegistryAccessOutOfScope {
                    bundle: projection.bundle.clone(),
                    projection: projection.projection.clone(),
                    registry,
                });
            }
        }
    }
    Ok(())
}

fn infer_action_effects(
    ir: &AspectModuleIr,
    action_names: &BTreeMap<String, BTreeSet<String>>,
    action_key: &str,
    visited: &mut BTreeSet<String>,
    effects: &mut BTreeSet<EffectKind>,
    writes: &mut BTreeSet<String>,
    registry_accesses: &mut BTreeSet<String>,
) {
    if !visited.insert(action_key.into()) {
        return;
    }
    let Some(action) = ir.actions.get(action_key) else {
        return;
    };
    if let Some(body) = &action.body {
        infer_expr_effects(
            ir,
            action_names,
            body,
            visited,
            effects,
            writes,
            registry_accesses,
        );
    }
}

fn infer_expr_effects(
    ir: &AspectModuleIr,
    action_names: &BTreeMap<String, BTreeSet<String>>,
    expr: &Expr,
    visited: &mut BTreeSet<String>,
    effects: &mut BTreeSet<EffectKind>,
    writes: &mut BTreeSet<String>,
    registry_accesses: &mut BTreeSet<String>,
) {
    let Expr::List(items) = expr else {
        return;
    };
    let Some(Expr::Atom(head)) = items.first() else {
        for item in items {
            infer_expr_effects(
                ir,
                action_names,
                item,
                visited,
                effects,
                writes,
                registry_accesses,
            );
        }
        return;
    };

    if head == "=" && items.len() == 3 {
        infer_expr_effects(
            ir,
            action_names,
            &items[2],
            visited,
            effects,
            writes,
            registry_accesses,
        );
        return;
    }

    match head.as_str() {
        "state-get" => {
            effects.insert(EffectKind::ReadState);
        }
        "state-set!" => {
            effects.insert(EffectKind::WriteState);
            if let Some(Expr::Atom(state)) = items.get(1) {
                writes.insert(state.clone());
            }
        }
        "registry-get" => {
            effects.insert(EffectKind::ReadRegistry);
            if let Some(Expr::Atom(registry)) = items.get(1) {
                registry_accesses.insert(registry.clone());
            }
        }
        "registry-set!" => {
            effects.insert(EffectKind::WriteRegistry);
            if let Some(Expr::Atom(registry)) = items.get(1) {
                registry_accesses.insert(registry.clone());
            }
        }
        "registry-consume!" => {
            effects.insert(EffectKind::ConsumeRegistryGrant);
            if let Some(Expr::Atom(registry)) = items.get(1) {
                registry_accesses.insert(registry.clone());
            }
        }
        "emit!" => {
            effects.insert(EffectKind::EmitEvent);
        }
        "call-contract!" => {
            effects.insert(EffectKind::CallContract);
        }
        "deploy-contract!" => {
            effects.insert(EffectKind::DeployContract);
        }
        "schedule-upgrade!" => {
            effects.insert(EffectKind::ScheduleUpgrade);
        }
        "execute-upgrade!" => {
            effects.insert(EffectKind::ExecuteUpgrade);
        }
        "cross-shard-outbox-append!" => {
            effects.insert(EffectKind::CrossShardOutboxAppend);
        }
        "permit-verify!" => {
            effects.insert(EffectKind::UsePermitCertificate);
        }
        "abort" => {
            effects.insert(EffectKind::Abort);
        }
        action_name => {
            if let Some(keys) = action_names.get(action_name) {
                for key in keys {
                    infer_action_effects(
                        ir,
                        action_names,
                        key,
                        visited,
                        effects,
                        writes,
                        registry_accesses,
                    );
                }
            }
        }
    }

    for item in items.iter().skip(1) {
        infer_expr_effects(
            ir,
            action_names,
            item,
            visited,
            effects,
            writes,
            registry_accesses,
        );
    }
}

fn action_for_projection_target<'a>(
    ir: &'a AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
    bundle: &str,
    target: &str,
) -> Option<&'a ActionDef> {
    let closure = closures.get(bundle)?;
    ir.actions
        .values()
        .find(|action| action.action == target && closure.contains(&action.aspect))
}

fn state_owned_by_bundle(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
    bundle: &str,
    state: &str,
) -> bool {
    let Some(closure) = closures.get(bundle) else {
        return false;
    };
    ir.storage_schema
        .values()
        .any(|schema| schema.state == state && closure.contains(&schema.aspect))
}

fn registry_owned_by_bundle(
    ir: &AspectModuleIr,
    closures: &BTreeMap<String, BTreeSet<String>>,
    bundle: &str,
    registry: &str,
) -> bool {
    let Some(closure) = closures.get(bundle) else {
        return false;
    };
    ir.registry_schema
        .values()
        .any(|schema| schema.registry == registry && closure.contains(&schema.aspect))
}

fn action_names(ir: &AspectModuleIr) -> BTreeMap<String, BTreeSet<String>> {
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for action in ir.actions.values() {
        names
            .entry(action.action.clone())
            .or_default()
            .insert(qualified(&action.aspect, &action.action));
    }
    names
}

fn collect_action_calls(
    expr: &Expr,
    action_names: &BTreeMap<String, BTreeSet<String>>,
    calls: &mut BTreeSet<String>,
) {
    let Expr::List(items) = expr else {
        return;
    };
    if let Some(Expr::Atom(head)) = items.first() {
        if head == "=" && items.len() == 3 {
            collect_action_calls(&items[2], action_names, calls);
            return;
        }
        if let Some(keys) = action_names.get(head) {
            calls.extend(keys.iter().cloned());
        }
    }
    for item in items.iter().skip(1) {
        collect_action_calls(item, action_names, calls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TRANSFER_TOKEN_FIXTURE: &str =
        include_str!("../../../models/aspects/stdlib/minimal-transfer-token.metta");

    #[test]
    fn parser_accepts_minimal_transfer_token_fixture() {
        let ast = parse_aspect_package(MINIMAL_TRANSFER_TOKEN_FIXTURE).unwrap();
        assert_eq!(ast.declarations.len(), 332);

        let canonical = canonical_aspect_source(&ast);
        assert!(canonical.contains("(bundle MinimalTransferToken)"));
        assert!(canonical.contains(
            "(projection MinimalTransferToken ERC20-transfer (= (ERC20.transfer $to $amount) (transferFromSelf tx.sender $to $amount)))"
        ));
        assert!(!canonical.contains(';'));
    }

    #[test]
    fn canonical_source_root_is_stable_under_whitespace_and_comments() {
        let source_a = "
            ; comments are not part of canonical source
            ( aspect BalanceAspect )
            (bundle Minimal)
            (: balanceOf (-> Address Amount))
        ";
        let source_b =
            "(aspect BalanceAspect)\n(bundle Minimal)\n(: balanceOf (-> Address Amount))";

        let ast_a = parse_aspect_package(source_a).unwrap();
        let ast_b = parse_aspect_package(source_b).unwrap();

        assert_eq!(
            canonical_aspect_source(&ast_a),
            canonical_aspect_source(&ast_b)
        );
        assert_eq!(canonical_source_root(&ast_a), canonical_source_root(&ast_b));
    }

    #[test]
    fn parser_rejects_unknown_top_level_declaration() {
        let error = parse_aspect_package("(unknown-form A B)").unwrap_err();
        assert_eq!(
            error,
            AspectError::UnknownDeclaration("unknown-form".into())
        );
    }

    #[test]
    fn parser_rejects_forbidden_top_level_form() {
        let error = parse_aspect_package("(add-atom &self (Balance Alice 1))").unwrap_err();
        assert_eq!(error, AspectError::ForbiddenForm("add-atom".into()));
    }

    #[test]
    fn parser_rejects_forbidden_nested_form() {
        let source = "
            (aspect MaliciousAspect)
            (action MaliciousAspect mutate)
            (derived MaliciousAspect mutate
              (add-atom &self (Balance Alice 1)))
        ";
        let error = parse_aspect_package(source).unwrap_err();
        assert_eq!(error, AspectError::ForbiddenForm("add-atom".into()));
    }

    #[test]
    fn parser_rejects_wrong_arity() {
        let error = parse_aspect_package("(bundle)").unwrap_err();
        assert_eq!(
            error,
            AspectError::InvalidArity {
                form: "bundle".into(),
                expected: 1,
                actual: 0,
            }
        );
    }

    #[test]
    fn string_literals_are_rejected_for_initial_declaration_subset() {
        let error = parse_aspect_package("(bundle \"Token\")").unwrap_err();
        assert_eq!(error, AspectError::StringLiteralUnsupported);
    }

    #[test]
    fn verifier_accepts_minimal_transfer_token_fixture() {
        let (canonical, ir, verified) =
            parse_verify_module(MINIMAL_TRANSFER_TOKEN_FIXTURE).unwrap();
        assert_eq!(ir.projections.len(), 39);
        assert_eq!(ir.abi.len(), 39);
        assert_eq!(ir.policies.len(), 39);

        let closure = verified
            .bundle_aspect_closures
            .get("MinimalTransferToken")
            .unwrap();
        assert!(closure.contains("BalanceAspect"));
        assert!(closure.contains("StaticBalanceAspect"));
        assert!(closure.contains("TransferableBalanceAspect"));
        assert!(closure.contains("SelfTransferAspect"));
        assert!(closure.contains("ApprovalAspect"));
        assert!(closure.contains("DelegatedTransferAspect"));
        let erc20_closure = verified
            .bundle_aspect_closures
            .get("ERC20ConformantToken")
            .unwrap();
        assert_eq!(closure, erc20_closure);
        let fee_closure = verified.bundle_aspect_closures.get("FeeToken").unwrap();
        assert!(fee_closure.contains("BalanceAspect"));
        assert!(fee_closure.contains("StaticBalanceAspect"));
        assert!(fee_closure.contains("TransferableBalanceAspect"));
        assert!(fee_closure.contains("FeeTransferAspect"));
        let pausable_closure = verified
            .bundle_aspect_closures
            .get("PausableToken")
            .unwrap();
        assert!(pausable_closure.contains("BalanceAspect"));
        assert!(pausable_closure.contains("StaticBalanceAspect"));
        assert!(pausable_closure.contains("TransferableBalanceAspect"));
        assert!(pausable_closure.contains("PausableTransferAspect"));
        let restricted_closure = verified
            .bundle_aspect_closures
            .get("RestrictedToken")
            .unwrap();
        assert!(restricted_closure.contains("BalanceAspect"));
        assert!(restricted_closure.contains("StaticBalanceAspect"));
        assert!(restricted_closure.contains("TransferableBalanceAspect"));
        assert!(restricted_closure.contains("RestrictedTransferAspect"));
        let locked_closure = verified.bundle_aspect_closures.get("LockedToken").unwrap();
        assert!(locked_closure.contains("BalanceAspect"));
        assert!(locked_closure.contains("StaticBalanceAspect"));
        assert!(locked_closure.contains("TransferableBalanceAspect"));
        assert!(locked_closure.contains("LockedTransferAspect"));
        let mint_burn_closure = verified
            .bundle_aspect_closures
            .get("MintBurnToken")
            .unwrap();
        assert!(mint_burn_closure.contains("BalanceAspect"));
        assert!(mint_burn_closure.contains("StaticBalanceAspect"));
        assert!(mint_burn_closure.contains("TransferableBalanceAspect"));
        assert!(mint_burn_closure.contains("MintableBalanceAspect"));
        assert!(mint_burn_closure.contains("BurnableBalanceAspect"));
        let capped_mint_closure = verified
            .bundle_aspect_closures
            .get("CappedMintToken")
            .unwrap();
        assert!(capped_mint_closure.contains("BalanceAspect"));
        assert!(capped_mint_closure.contains("StaticBalanceAspect"));
        assert!(capped_mint_closure.contains("TransferableBalanceAspect"));
        assert!(capped_mint_closure.contains("MintableBalanceAspect"));
        assert!(capped_mint_closure.contains("CappedMintableAspect"));
        let votable_closure = verified.bundle_aspect_closures.get("VotableToken").unwrap();
        assert!(votable_closure.contains("BalanceAspect"));
        assert!(votable_closure.contains("StaticBalanceAspect"));
        assert!(votable_closure.contains("TransferableBalanceAspect"));
        assert!(votable_closure.contains("VotableBalanceAspect"));
        let snapshot_closure = verified
            .bundle_aspect_closures
            .get("SnapshotToken")
            .unwrap();
        assert!(snapshot_closure.contains("BalanceAspect"));
        assert!(snapshot_closure.contains("StaticBalanceAspect"));
        assert!(snapshot_closure.contains("TransferableBalanceAspect"));
        assert!(snapshot_closure.contains("SnapshotBalanceAspect"));
        let vault_closure = verified
            .bundle_aspect_closures
            .get("VaultShareToken")
            .unwrap();
        assert!(vault_closure.contains("BalanceAspect"));
        assert!(vault_closure.contains("VaultShareBalanceAspect"));

        let artifact = module_artifact("MinimalTransferToken", canonical.clone(), &verified);
        assert_eq!(artifact.source_root, ir.source_root);
        assert_eq!(artifact.ir_root, verified.roots.ir_root);
        assert_eq!(artifact.abi_root, verified.roots.abi_root);
        assert_eq!(artifact.policy_root, verified.roots.policy_root);
        assert_eq!(
            artifact.registry_schema_root,
            verified.roots.registry_schema_root
        );

        let (_, _, second_verified) = parse_verify_module(MINIMAL_TRANSFER_TOKEN_FIXTURE).unwrap();
        assert_eq!(verified.roots, second_verified.roots);
    }

    #[test]
    fn verifier_rejects_duplicate_state_ownership() {
        let source = "
            (: Address Type)
            (: Amount Type)
            (aspect BalanceA)
            (aspect BalanceB)
            (owns BalanceA balanceOf (-> Address Amount))
            (owns BalanceB balanceOf (-> Address Amount))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::DuplicateStateOwnership {
                state: "balanceOf".into(),
                first_aspect: "BalanceA".into(),
                second_aspect: "BalanceB".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_duplicate_registry_ownership() {
        let source = "
            (: Amount Type)
            (aspect ApprovalA)
            (aspect ApprovalB)
            (registry-owns ApprovalA allowance Amount)
            (registry-owns ApprovalB allowance Amount)
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::DuplicateRegistryOwnership {
                registry: "allowance".into(),
                first_aspect: "ApprovalA".into(),
                second_aspect: "ApprovalB".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_conflicting_bundle_aspects() {
        let source = "
            (aspect FeeTransferAspect)
            (aspect RebasableBalanceAspect)
            (conflicts FeeTransferAspect RebasableBalanceAspect)
            (bundle BadBundle)
            (bundle-includes BadBundle FeeTransferAspect)
            (bundle-includes BadBundle RebasableBalanceAspect)
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::ConflictingAspects {
                bundle: "BadBundle".into(),
                aspect: "FeeTransferAspect".into(),
                other: "RebasableBalanceAspect".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_missing_required_facet() {
        let source = "
            (aspect SelfTransferAspect)
            (requires SelfTransferAspect Balance-Transfer)
            (bundle BadBundle)
            (bundle-includes BadBundle SelfTransferAspect)
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::MissingRequiredFacet {
                bundle: "BadBundle".into(),
                aspect: "SelfTransferAspect".into(),
                facet: "Balance-Transfer".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_projection_without_abi() {
        let source = "
            (aspect CallAspect)
            (action CallAspect call)
            (derived CallAspect call (= (call) True))
            (bundle BadBundle)
            (bundle-includes BadBundle CallAspect)
            (projection BadBundle PublicCall (= (API.call) (call)))
            (method-policy BadBundle PublicCall TxSender (effects ReadState) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::MissingProjectionAbi {
                bundle: "BadBundle".into(),
                projection: "PublicCall".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_projection_without_policy() {
        let source = "
            (aspect CallAspect)
            (action CallAspect call)
            (derived CallAspect call (= (call) True))
            (bundle BadBundle)
            (bundle-includes BadBundle CallAspect)
            (projection BadBundle PublicCall (= (API.call) (call)))
            (method-abi BadBundle PublicCall (args) Bool)
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::MissingProjectionPolicy {
                bundle: "BadBundle".into(),
                projection: "PublicCall".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_projection_target_outside_bundle_closure() {
        let source = "
            (aspect InternalAspect)
            (action InternalAspect call)
            (derived InternalAspect call (= (call) True))
            (aspect PublicAspect)
            (bundle BadBundle)
            (bundle-includes BadBundle PublicAspect)
            (projection BadBundle PublicCall (= (API.call) (call)))
            (method-abi BadBundle PublicCall (args) Bool)
            (method-policy BadBundle PublicCall TxSender (effects ReadState) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::ProjectionTargetUnsupported {
                bundle: "BadBundle".into(),
                projection: "PublicCall".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_undeclared_inferred_effect() {
        let source = "
            (: Bool Type)
            (: Amount Type)
            (aspect WriterAspect)
            (owns WriterAspect balance Amount)
            (action WriterAspect writeBalance)
            (derived WriterAspect writeBalance
              (= (writeBalance $amount)
                 (state-set! balance $amount)))
            (bundle BadBundle)
            (bundle-includes BadBundle WriterAspect)
            (projection BadBundle Write (= (API.write $amount) (writeBalance $amount)))
            (method-abi BadBundle Write (args (amount Amount)) Bool)
            (method-policy BadBundle Write TxSender (effects ReadState) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::UndeclaredEffect {
                bundle: "BadBundle".into(),
                projection: "Write".into(),
                effect: EffectKind::WriteState,
            }
        );
    }

    #[test]
    fn verifier_accepts_declared_state_write_in_bundle_scope() {
        let source = "
            (: Bool Type)
            (: Amount Type)
            (aspect WriterAspect)
            (owns WriterAspect balance Amount)
            (action WriterAspect writeBalance)
            (derived WriterAspect writeBalance
              (= (writeBalance $amount)
                 (state-set! balance $amount)))
            (bundle GoodBundle)
            (bundle-includes GoodBundle WriterAspect)
            (projection GoodBundle Write (= (API.write $amount) (writeBalance $amount)))
            (method-abi GoodBundle Write (args (amount Amount)) Bool)
            (method-policy GoodBundle Write TxSender (effects WriteState) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert!(verify_module(&ir).is_ok());
    }

    #[test]
    fn verifier_rejects_state_write_outside_bundle_scope() {
        let source = "
            (: Bool Type)
            (: Amount Type)
            (aspect StorageAspect)
            (owns StorageAspect balance Amount)
            (aspect WriterAspect)
            (action WriterAspect writeBalance)
            (derived WriterAspect writeBalance
              (= (writeBalance $amount)
                 (state-set! balance $amount)))
            (bundle BadBundle)
            (bundle-includes BadBundle WriterAspect)
            (projection BadBundle Write (= (API.write $amount) (writeBalance $amount)))
            (method-abi BadBundle Write (args (amount Amount)) Bool)
            (method-policy BadBundle Write TxSender (effects WriteState) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::StateWriteOutOfScope {
                bundle: "BadBundle".into(),
                projection: "Write".into(),
                state: "balance".into(),
            }
        );
    }

    #[test]
    fn verifier_rejects_action_call_cycles() {
        let source = "
            (aspect LoopAspect)
            (action LoopAspect first)
            (action LoopAspect second)
            (derived LoopAspect first (= (first) (second)))
            (derived LoopAspect second (= (second) (first)))
            (bundle LoopBundle)
            (bundle-includes LoopBundle LoopAspect)
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert!(matches!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::ReentrantActionCycle { .. }
        ));
    }

    #[test]
    fn verifier_rejects_unknown_policy_effect() {
        let source = "
            (aspect CallAspect)
            (action CallAspect call)
            (derived CallAspect call (= (call) True))
            (bundle BadBundle)
            (bundle-includes BadBundle CallAspect)
            (projection BadBundle PublicCall (= (API.call) (call)))
            (method-abi BadBundle PublicCall (args) Bool)
            (method-policy BadBundle PublicCall TxSender (effects Teleport) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        assert_eq!(
            lower_to_ir(&ast).unwrap_err(),
            AspectVerifyError::UnsupportedEffect("Teleport".into())
        );
    }

    #[test]
    fn verifier_rejects_undeclared_registry_consumption() {
        let source = "
            (: Bool Type)
            (aspect AllowanceAspect)
            (action AllowanceAspect spend)
            (derived AllowanceAspect spend
              (= (spend)
                 (registry-consume! allowance 1)))
            (bundle BadBundle)
            (bundle-includes BadBundle AllowanceAspect)
            (projection BadBundle Spend (= (API.spend) (spend)))
            (method-abi BadBundle Spend (args) Bool)
            (method-policy BadBundle Spend TxSender (effects ReadRegistry) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::UndeclaredEffect {
                bundle: "BadBundle".into(),
                projection: "Spend".into(),
                effect: EffectKind::ConsumeRegistryGrant,
            }
        );
    }

    #[test]
    fn verifier_accepts_declared_registry_consumption_in_bundle_scope() {
        let source = "
            (: Bool Type)
            (: Amount Type)
            (aspect AllowanceAspect)
            (registry-owns AllowanceAspect allowance Amount)
            (action AllowanceAspect spend)
            (derived AllowanceAspect spend
              (= (spend)
                 (registry-consume! allowance 1)))
            (bundle GoodBundle)
            (bundle-includes GoodBundle AllowanceAspect)
            (projection GoodBundle Spend (= (API.spend) (spend)))
            (method-abi GoodBundle Spend (args) Bool)
            (method-policy GoodBundle Spend TxSender (effects ConsumeRegistryGrant) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert!(verify_module(&ir).is_ok());
    }

    #[test]
    fn verifier_rejects_registry_consumption_outside_bundle_scope() {
        let source = "
            (: Bool Type)
            (: Amount Type)
            (aspect AllowanceStorage)
            (registry-owns AllowanceStorage allowance Amount)
            (aspect AllowanceConsumer)
            (action AllowanceConsumer spend)
            (derived AllowanceConsumer spend
              (= (spend)
                 (registry-consume! allowance 1)))
            (bundle BadBundle)
            (bundle-includes BadBundle AllowanceConsumer)
            (projection BadBundle Spend (= (API.spend) (spend)))
            (method-abi BadBundle Spend (args) Bool)
            (method-policy BadBundle Spend TxSender (effects ConsumeRegistryGrant) (invariants))
        ";
        let ast = parse_aspect_package(source).unwrap();
        let ir = lower_to_ir(&ast).unwrap();
        assert_eq!(
            verify_module(&ir).unwrap_err(),
            AspectVerifyError::RegistryAccessOutOfScope {
                bundle: "BadBundle".into(),
                projection: "Spend".into(),
                registry: "allowance".into(),
            }
        );
    }
}
