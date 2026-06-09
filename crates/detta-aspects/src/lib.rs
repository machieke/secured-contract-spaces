use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TRANSFER_TOKEN_FIXTURE: &str =
        include_str!("../../../models/aspects/stdlib/minimal-transfer-token.metta");

    #[test]
    fn parser_accepts_minimal_transfer_token_fixture() {
        let ast = parse_aspect_package(MINIMAL_TRANSFER_TOKEN_FIXTURE).unwrap();
        assert_eq!(ast.declarations.len(), 36);

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
}
