// TODO(ed): Remove me
#![allow(unused)]

use std::path::{PathBuf, Path};
use std::collections::HashMap;
use crate::error::Error;
use crate::tokenizer::Token as T;
use crate::Type as RuntimeType;
use crate::compiler::Prec;
use crate::Next;

#[derive(Debug, Copy, Clone)]
pub struct Span {
    // TODO(ed): Do this more intelligent, so
    // we can show ranges. Maybe even go back
    // to offsets from start of the file.
    line: usize,
}

#[derive(Debug, Clone)]
pub struct Prog {
    files: Vec<(PathBuf, Module)>,
}

#[derive(Debug, Clone)]
pub struct Module {
    span: Span,
    statements: Vec<Statement>,
}

#[derive(Debug, Copy, Clone)]
pub enum VarKind {
    Const,
    Mutable,
    GlobalConst,
    GlobalMutable,
}

#[derive(Debug, Copy, Clone)]
pub enum AssignmentOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Use {
        file: Identifier,
    },

    Blob {
        name: Identifier,
        fields: HashMap<Identifier, Type>,
    },

    Print {
        expr: Expression,
    },

    Assignment {
        target: Assignable,
        kind: AssignmentOp,
        value: Expression,
    },

    Definition {
        ident: Identifier,
        value: Expression,
        kind: VarKind,
    },

    If {
        condition: Expression,
        pass: Vec<Statement>,
        fail: Vec<Statement>,
    },

    Loop {
        condition: Expression,
        body: Vec<Statement>,
    },

    Ret {
        value: Option<Expression>,
    },

    Block {
        statements: Vec<Statement>,
    },

    Assert {
        expression: Expression,
    },

    StatementExpression {
        value: Expression,
    },
}

#[derive(Debug, Clone)]
pub struct Statement {
    span: Span,
    kind: StatementKind,
}

#[derive(Debug, Clone)]
pub struct Identifier {
    span: Span,
    name: String,
}

#[derive(Debug, Clone)]
pub enum AssignableKind {
    Read(Identifier),
    Call(Box<Assignable>, Vec<Expression>),
    Access(Box<Assignable>, Box<Assignable>),
    Index(Box<Assignable>, Box<Expression>),
}

#[derive(Debug, Clone)]
pub struct Assignable {
    span: Span,
    kind: AssignableKind,
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Get(Assignable),

    Add(Box<Expression>, Box<Expression>),
    Sub(Box<Expression>, Box<Expression>),
    Mul(Box<Expression>, Box<Expression>),
    Div(Box<Expression>, Box<Expression>),
    Neg(Box<Expression>),

    Eq(Box<Expression>, Box<Expression>),
    Neq(Box<Expression>, Box<Expression>),
    Gt(Box<Expression>, Box<Expression>),
    Gteq(Box<Expression>, Box<Expression>),
    Lt(Box<Expression>, Box<Expression>),
    Lteq(Box<Expression>, Box<Expression>),
    AssertEq(Box<Expression>, Box<Expression>),

    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Not(Box<Expression>),

    // Composite
    Function {
        name: Identifier,
        args: Vec<(Identifier, Type)>,
        ret: Type,

        body: Box<Statement>,
    },
    Tuple(Vec<Expression>),
    List(Vec<Expression>),
    Set(Vec<Expression>),
    // Has to have even length, listed { k1, v1, k2, v2 }
    Dict(Vec<Expression>),

    // Simple
    Float(f64),
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone)]
pub struct Expression {
    span: Span,
    kind: ExpressionKind,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Implied,
    Union(Box<Type>, Box<Type>),
    Resolved(RuntimeType),
    Fn(Vec<Type>, Box<Type>),
    Unresolved(String),
}

#[derive(Debug, Clone)]
pub struct Type {
    span: Span,
    kind: TypeKind,
}

type Tokens = [(T, usize)];
type ParseResult<'t, T> = Result<(Context<'t>, T), (Context<'t>,  Vec<Error>)>;

#[derive(Debug, Copy, Clone)]
struct Context<'a> {
    pub tokens: &'a Tokens,
    pub curr: usize,
    pub file: &'a Path,
}

impl<'a> Context<'a> {

    fn new(tokens: &'a [(T, usize)], file: &'a Path) -> Self {
        Self { tokens, curr: 0, file, }
    }

    fn span(&self) -> Span {
        Span { line: self.peek().1 }
    }

    fn line(&self) -> usize {
        self.span().line
    }

    fn skip(&self, n: usize) -> Self {
        let mut new = self.clone();
        new.curr += n;
        new
    }

    fn peek(&self) -> &(T, usize) {
        &self.tokens.get(self.curr).unwrap_or(&(T::EOF, 0))
    }

    fn token(&self) -> &T {
        &self.peek().0
    }

}

macro_rules! eat {
    ($ctx:expr) => {
        ($ctx.token(), $ctx.span(), $ctx.skip(1))
    }
}

macro_rules! syntax_error {
    ($ctx:expr, $( $msg:expr ),* ) => {
        {
            let msg = format!($( $msg ),*).into();
            Error::SyntaxError {
                file: $ctx.file.to_path_buf(),
                line: $ctx.line(),
                token: $ctx.token().clone(),
                message: Some(msg),
            }
        }
    };
}

macro_rules! raise_syntax_error {
    ($ctx:expr, $( $msg:expr ),* ) => {
        return Err(($ctx.skip(1), vec![syntax_error!($ctx, $( $msg ),*)]))
    };
}

macro_rules! expect {
    ($ctx:expr, $( $token:pat )|+ , $( $msg:expr ),+ ) => {
        {
            if !matches!($ctx.token(), $( $token )|* ) {
                raise_syntax_error!($ctx, $( $msg ),*);
            }
            $ctx.skip(1)
        }
    };

    ($ctx:expr, $( $token:pat )|+) => {
        expect!($ctx, $( $token )|*, concat!("Expected ", stringify!($( $token )|*)))
    };
}

macro_rules! skip_if {
    ($ctx:expr, $( $token:pat )|+ ) => {
        {
            if matches!($ctx.token(), $( $token )|* ) {
                $ctx.skip(1)
            } else {
                $ctx
            }
        }
    };
}

fn parse_type<'t>(ctx: Context<'t>) -> ParseResult<'t, Type> {
    use RuntimeType::{Void, Int, Float, Bool, String};
    use TypeKind::*;
    let span = ctx.span();
    let (ctx, kind) = match ctx.token() {
        T::Identifier(name) => {
            (ctx.skip(1), match name.as_str() {
                "void" => Resolved(Void),
                "int" => Resolved(Int),
                "float" => Resolved(Float),
                "bool" => Resolved(Bool),
                "str" => Resolved(String),
                _ => Unresolved(name.clone()),
            })
        }

        T::Fn => {
            let mut ctx = ctx.skip(1);
            let mut params = Vec::new();
            let ret = loop {
                match ctx.token() {
                    T::Arrow => {
                        ctx = ctx.skip(1);
                        break if let Ok((_ctx, ret)) = parse_type(ctx) {
                            ctx = _ctx;
                            ret
                        } else {
                            Type { span: ctx.span(), kind: Resolved(Void) }
                        };
                    }

                    _ => {
                        let (_ctx, param) = parse_type(ctx)?;
                        ctx = _ctx;
                        params.push(param);

                        ctx = if matches!(ctx.token(), T::Comma | T::Arrow) {
                            skip_if!(ctx, T::Comma)
                        } else {
                            raise_syntax_error!(ctx, "Expected ',' or '->' after type parameter")
                        };
                    }

                    T::EOF => {
                        raise_syntax_error!(ctx, "Didn't expect EOF in type definition");
                    }
                }
            };

            (ctx, Fn(params, Box::new(ret)))
        }

        t => {
            raise_syntax_error!(ctx, "No type starts with '{:?}'", t);
        }
    };

    let ty = Type { span, kind };

    let (ctx, ty) = if matches!(ctx.token(), T::Pipe) {
        let (ctx, rest) = parse_type(ctx.skip(1))?;
        (ctx, Type { span, kind: Union(Box::new(ty), Box::new(rest)) })
    } else {
        (ctx, ty)
    };

    let (ctx, ty) = if matches!(ctx.token(), T::QuestionMark) {
        use RuntimeType::Void;
        let void = Type { span: ctx.span(), kind: Resolved(Void) };
        (ctx.skip(1), Type { span, kind: Union(Box::new(ty), Box::new(void)) })
    } else {
        (ctx, ty)
    };

    Ok((ctx, ty))
}

fn expression<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
    use ExpressionKind::*;

    fn function<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
        unimplemented!("Function parsing is not implemented");
    }

    fn parse_precedence<'t>(ctx: Context<'t>, prec: Prec) -> ParseResult<'t, Expression> {
        fn precedence(token: &T) -> Prec {
            match token {
                T::LeftBracket => Prec::Index,

                T::Star | T::Slash => Prec::Factor,

                T::Minus | T::Plus => Prec::Term,

                T::EqualEqual
                | T::Greater
                | T::GreaterEqual
                | T::Less
                | T::LessEqual
                | T::NotEqual => Prec::Comp,

                T::And => Prec::BoolAnd,
                T::Or => Prec::BoolOr,

                T::AssertEqual => Prec::Assert,

                T::Arrow => Prec::Arrow,

                _ => Prec::No,
            }
        }

        fn value<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            let (token, span, ctx) = eat!(ctx);
            let kind = match token.clone() {
                T::Float(f) => Float(f),
                T::Int(i) => Int(i),
                T::Bool(b) => Bool(b),
                T::Nil => Nil,
                T::String(s) => Str(s),
                t => {
                    raise_syntax_error!(ctx, "Cannot parse value, '{:?}' is not a valid value", t);
                }
            };
            Ok((ctx, Expression { span, kind }))
        }

        fn prefix<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            match ctx.token() {
                T::LeftParen => grouping_or_tuple(ctx),
                T::LeftBracket => list(ctx),
                T::LeftBrace => set_or_dict(ctx),

                T::Float(_) | T::Int(_) | T::Bool(_) | T::String(_) | T::Nil => value(ctx),
                T::Minus | T::Bang => unary(ctx),

                T::Identifier(_) => {
                    let span = ctx.span();
                    let (ctx, assign) = assignable(ctx)?;
                    Ok((ctx, Expression { span, kind: Get(assign) }))
                }

                t => {
                    raise_syntax_error!(ctx, "No valid expression starts with '{:?}'", t);
                }
            }
        }

        fn unary<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            let (op, span, ctx) = eat!(ctx);
            let (ctx, expr) = parse_precedence(ctx, Prec::Factor)?;
            let expr = Box::new(expr);

            let kind = match op {
                T::Minus => Neg(expr),
                T::Bang => Not(expr),

                _ => {
                    raise_syntax_error!(ctx, "Invalid unary operator");
                }
            };
            Ok((ctx, Expression { span, kind }))
        }

        fn infix<'t>(ctx: Context<'t>, lhs: &Expression) -> ParseResult<'t, Expression> {
            let (op, span, ctx) = eat!(ctx);

            let (ctx, rhs) = parse_precedence(ctx, precedence(op).next())?;

            let lhs = Box::new(lhs.clone());
            let rhs = Box::new(rhs);

            let kind = match op {
                T::Plus => Add(lhs, rhs),
                T::Minus => Sub(lhs, rhs),
                T::Star => Mul(lhs, rhs),
                T::Slash => Div(lhs, rhs),
                T::EqualEqual => Eq(lhs, rhs),
                T::NotEqual => Neq(lhs, rhs),
                T::Greater => Gt(lhs, rhs),
                T::GreaterEqual => Gteq(lhs, rhs),
                T::Less => Lt(lhs, rhs),
                T::LessEqual => Lteq(lhs, rhs),

                T::And => And(lhs, rhs),
                T::Or => Or(lhs, rhs),

                T::AssertEqual => AssertEq(lhs, rhs),

                T::Arrow => {
                    use AssignableKind::*;
                    if let Expression { kind: Get(Assignable { kind: Call(calle, mut args), .. }), span } = *rhs {
                        args.insert(0, *lhs);
                        Get(Assignable { kind: Call(calle, args), span })
                    } else {
                        raise_syntax_error!(ctx, "Expected a call-expression after '->'");
                    }
                },

                _ => {
                    return Err((ctx, Vec::new()));
                }
            };
            Ok((ctx, Expression { span, kind }))
        }

        fn maybe_call<'t>(ctx: Context<'t>, calle: Assignable) -> ParseResult<'t, Assignable> {
            if !matches!(ctx.token(), T::LeftParen | T::Bang) {
                return Ok((ctx, calle))
            }

            let span = ctx.span();
            let banger = matches!(ctx.token(), T::Bang);
            let mut ctx = expect!(ctx, T::Bang | T::LeftParen, "Expected '(' or '!' when calling function");
            let mut args = Vec::new();

            loop {
                match (ctx.token(), banger) {
                    (T::EOF, _)
                    | (T::RightParen, false)
                    | (T::Dot, true)
                    | (T::Newline, true)
                    | (T::Arrow, true)
                        => { break; }

                    _ => {
                        let (_ctx, expr) = expression(ctx)?;
                        ctx = _ctx;
                        args.push(expr);

                        ctx = skip_if!(ctx, T::Comma);
                    }
                }
            }

            let ctx = if !banger {
                expect!(ctx, T::RightParen, "Expected ')' after calling function")
            } else {
                ctx
            };

            use AssignableKind::Call;
            let result = Assignable { span, kind: Call(Box::new(calle), args) };
            maybe_call(ctx, result)
        }

        fn assignable<'t>(ctx: Context<'t>) -> ParseResult<'t, Assignable> {
            use AssignableKind::*;

            let ident = if let (T::Identifier(name), span) = (ctx.token(), ctx.span()) {
                Assignable { span, kind: Read(Identifier { span, name: name.clone() })}
            } else {
                raise_syntax_error!(ctx, "Assignable expressions have to start with an identifier");
            };

            let (ctx, ident) = maybe_call(ctx.skip(1), ident)?;
            let span = ctx.span();
            let result = match ctx.token() {
                T::Dot => {
                    let (ctx, rest) = assignable(ctx.skip(1))?;
                    let kind = Access(Box::new(ident), Box::new(rest));
                    (ctx, Assignable { span, kind })
                }

                T::LeftBracket => {
                    let (ctx, index) = expression(ctx.skip(1))?;
                    (ctx.skip(1), Assignable { span, kind: Index(Box::new(ident), Box::new(index))})
                }

                _ => {
                    (ctx, ident)
                }
            };
            Ok(result)
        }

        fn grouping_or_tuple<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            let span = ctx.span();
            let ctx = expect!(ctx, T::LeftParen, "Expected '('");


            let (mut ctx, expr) = expression(ctx)?;
            let mut exprs = vec![expr];

            let tuple = matches!(ctx.token(), T::Comma);
            while tuple {
                ctx = skip_if!(ctx, T::Comma);
                match ctx.token() {
                    T::EOF | T::RightParen => {
                        break;
                    }

                    _ => {
                        let (_ctx, expr) = expression(ctx)?;
                        exprs.push(expr);
                        ctx = _ctx;
                    }
                }
            }

            ctx = expect!(ctx, T::RightParen, "Expected ')'");
            let result = if tuple {
                Expression { span, kind: Tuple(exprs) }
            } else {
                exprs.into_iter().next().unwrap()
            };
            Ok((ctx, result))
        }

        fn list<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            let span = ctx.span();
            let mut ctx = expect!(ctx, T::LeftBracket, "Expected '['");

            let mut exprs = Vec::new();
            loop {
                match ctx.token() {
                    T::EOF | T::RightBracket => {
                        break;
                    }

                    _ => {
                        let (_ctx, expr) = expression(ctx)?;
                        exprs.push(expr);
                        ctx = skip_if!(_ctx, T::Comma);
                    }
                }
            }

            ctx = expect!(ctx, T::RightBracket, "Expected ']'");
            Ok((ctx, Expression { span, kind: List(exprs) }))
        }

        fn set_or_dict<'t>(ctx: Context<'t>) -> ParseResult<'t, Expression> {
            let span = ctx.span();
            let mut ctx = expect!(ctx, T::LeftBrace, "Expected '{{'");

            // NOTE(ed): I decided on {:} for empty dicts, and {} for empty sets.
            let mut exprs = Vec::new();
            let mut is_dict = None;
            loop {
                match ctx.token() {
                    T::EOF | T::RightBrace => {
                        break;
                    }

                    T::Colon => {
                        if is_dict.is_some() {
                            raise_syntax_error!(ctx, "Didn't expect empty dict pair since it has to be a {}",
                                if is_dict.unwrap() { "dict" } else { "set" }
                            );
                        }
                        is_dict = Some(true);
                        ctx = ctx.skip(1);
                    }

                    _ => {
                        let (_ctx, expr) = expression(ctx)?;
                        ctx = _ctx;
                        exprs.push(expr);

                        is_dict = if is_dict.is_none() {
                            Some(matches!(ctx.token(), T::Colon))
                        } else {
                            is_dict
                        };

                        if is_dict.unwrap() {
                            ctx = expect!(ctx, T::Colon, "Expected ':' for dict pair");
                            let (_ctx, expr) = expression(ctx)?;
                            ctx = _ctx;
                            exprs.push(expr);
                        }

                        ctx = skip_if!(ctx, T::Comma);
                    }
                }
            }

            ctx = expect!(ctx, T::RightBrace, "Expected '}}'");

            let kind = if is_dict.unwrap_or(false) {
                Dict(exprs)
            } else {
                Set(exprs)
            };

            Ok((ctx, Expression { span, kind }))
        }

        let pre = prefix(ctx);
        if let Err((ctx, mut errs)) = pre {
            errs.push(syntax_error!(ctx, "Invalid expression"));
            return Err((ctx, errs));
        }

        let (mut ctx, mut expr) = pre.unwrap();
        while prec <= precedence(ctx.token()) {
            if let Ok((_ctx, _expr)) = infix(ctx, &expr) {
                ctx = _ctx;
                expr = _expr;
            } else {
                break;
            }
        }
        Ok((ctx, expr))
    }

    match ctx.token() {
        T::Fn => function(ctx),
        _ => parse_precedence(ctx, Prec::No),
    }
}

fn outer_statement<'t>(ctx: Context<'t>) -> ParseResult<Statement> {
    let span = ctx.span();
    let (ctx, value) = expression(ctx)?;

    let ctx = expect!(ctx, T::Newline, "Expected newline after statement");

    use StatementKind::*;
    Ok((ctx, Statement { span, kind: StatementExpression { value } }))
}

pub fn construct(tokens: &Tokens) -> Result<Module, Vec<Error>> {
    let path = PathBuf::from("hello.sy");
    let mut ctx = Context::new(tokens, &path);
    let mut errors = Vec::new();
    let mut statements = Vec::new();
    while !matches!(ctx.token(), T::EOF) {
        if matches!(ctx.token(), T::Newline) {
            ctx = ctx.skip(1);
            continue;
        }
        ctx = match outer_statement(ctx) {
            Ok((_ctx, statement)) => {
                statements.push(statement);
                _ctx
            }
            Err((_ctx, mut errs)) => {
                errors.append(&mut errs);
                _ctx
            }
        }
    }

    if errors.is_empty() {
        Ok(Module { span: Span { line: 0 }, statements })
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod test {
    use crate::tokenizer::string_to_tokens;
    use super::*;
    use ExpressionKind::*;
    use AssignableKind::*;
    use TypeKind::*;
    use RuntimeType as RT;

    macro_rules! test {
        ($f:ident, $name:ident: $str:expr => $ans:pat) => {
            #[test]
            fn $name () {
                let tokens = string_to_tokens($str);
                let path = PathBuf::from(stringify!($name));
                let result = $f(Context::new(&tokens, &path));
                assert!(result.is_ok(),
                    "\nSyntax tree test didn't parse for:\n{}\nErrs: {:?}",
                    $str,
                    result.unwrap_err().1
                );
                let (ctx, result) = result.unwrap();
                assert!(matches!(result.kind, $ans), "\nExpected: {}, but got: {:?}", stringify!($ans), result);
                assert_eq!(ctx.curr, ctx.tokens.len(), "Parsed too few or too many tokens:\n{}", $str);
            }
        }
    }

    // TODO(ed): It's really hard to write good tests, Rust refuses to deref the boxes
    // automatically.
    test!(expression, value: "0" => Int(0));
    test!(expression, add: "0 + 1.0" => Add(_, _));
    test!(expression, mul: "\"abc\" * \"abc\"" => Mul(_, _));
    test!(expression, ident: "a" => Get(Assignable { kind: Read(_), .. }));
    test!(expression, access: "a.b" => Get(Assignable { kind: Access(_, _), .. }));
    test!(expression, index_ident: "a[a]" => Get(Assignable { kind: Index(_, _), .. }));
    test!(expression, index_expr: "a[1 + 2 + 3]" => Get(Assignable { kind: Index(_, _), .. }));
    test!(expression, grouping: "(0 * 0) + 1" => Add(_, _));
    test!(expression, tuple: "(0, 0)" => Tuple(_));
    test!(expression, list: "[0, 0]" => List(_));
    test!(expression, set: "{1, 1}" => Set(_));
    test!(expression, dict: "{1: 1}" => Dict(_));
    test!(expression, zero_set: "{}" => Set(_));
    test!(expression, zero_dict: "{:}" => Dict(_));

    test!(expression, call_simple_paren: "a()" => Get(_));
    test!(expression, call_simple_bang: "a!" => Get(_));
    test!(expression, call_chaining_paren: "a().b" => Get(_));
    test!(expression, call_chaining_bang: "a!.b" => Get(_));
    test!(expression, call_args_paren: "a(1, 2, 3)" => Get(_));
    test!(expression, call_args_bang: "a! 1, 2, 3" => Get(_));
    test!(expression, call_args_chaining_paren: "a(1, 2, 3).b" => Get(_));
    test!(expression, call_args_chaining_paren_trailing: "a(1, 2, 3,).b" => Get(_));
    test!(expression, call_args_chaining_bang: "a! 1, 2, 3 .b" => Get(_));
    test!(expression, call_args_chaining_bang_trailing: "a! 1, 2, 3, .b" => Get(_));

    test!(expression, call_arrow: "1 + 0 -> a! 2, 3" => Add(_, _));
    test!(expression, call_arrow_grouping: "(1 + 0) -> a! 2, 3" => Get(_));

    test!(parse_type, type_void: "void" => Resolved(RT::Void));
    test!(parse_type, type_int: "int" => Resolved(RT::Int));
    test!(parse_type, type_float: "float" => Resolved(RT::Float));
    test!(parse_type, type_str: "str" => Resolved(RT::String));
    test!(parse_type, type_unknown: "blargh" => Unresolved(_));
    test!(parse_type, type_union: "int | int" => Union(_, _));
    test!(parse_type, type_question: "int?" => Union(_, _));
    test!(parse_type, type_union_and_question: "int | void | str?" => Union(_, _));
    test!(parse_type, type_fn_no_params: "fn ->" => Fn(_, _));
    test!(parse_type, type_fn_one_param: "fn int? -> bool" => Fn(_, _));
    test!(parse_type, type_fn_two_params: "fn int | void, int? -> str?" => Fn(_, _));
    test!(parse_type, type_fn_only_ret: "fn -> bool?" => Fn(_, _));
}
