use std::fmt::{self, Write};
use std::path::PathBuf;
use sylt_common::{Error, Type as RuntimeType};
use sylt_parser::statement::NameIdentifier;
use sylt_parser::{
    Assignable, AssignableKind, Expression, ExpressionKind, Identifier, Module, Op, Statement,
    StatementKind, Type, TypeKind, VarKind,
};

use crate::Args;

static INDENT: &'static str = "    ";

macro_rules! write_comma_separated {
    ($dest:expr, $indent:expr, $write:expr, $values:expr) => {
        let mut first = true;
        for value in $values {
            if !first {
                write!($dest, ", ")?;
            }
            first = false;
            $write($dest, $indent, value)?;
        }
    };
}

fn write_indents<W: Write>(dest: &mut W, indent: u32) -> fmt::Result {
    for _ in 0..indent {
        write!(dest, "{}", INDENT)?;
    }
    Ok(())
}

fn write_identifier<W: Write>(dest: &mut W, identifier: &Identifier) -> fmt::Result {
    write!(dest, "{}", identifier.name)
}

fn write_parameters<W: Write>(
    dest: &mut W,
    indent: u32,
    parameters: &[(Identifier, Type)],
) -> fmt::Result {
    let mut first = true;
    for (identifier, ty) in parameters {
        if !first {
            write!(dest, ", ")?;
        }
        first = false;
        write_identifier(dest, identifier)?;
        write!(dest, ": ")?;
        write_type(dest, indent, ty)?;
    }
    Ok(())
}

fn write_blob_instance_fields<W: Write>(
    dest: &mut W,
    indent: u32,
    fields: &[(String, Expression)],
) -> fmt::Result {
    for (field, expr) in fields {
        write_indents(dest, indent)?;
        write!(dest, "{}: ", field)?;
        write_expression(dest, indent, expr)?;
        write!(dest, "\n")?;
    }
    Ok(())
}

fn write_type<W: Write>(dest: &mut W, indent: u32, ty: &Type) -> fmt::Result {
    match &ty.kind {
        TypeKind::Implied => unreachable!(),
        TypeKind::Resolved(ty) => write!(dest, "{}", ty),
        TypeKind::UserDefined(assignable) => write_assignable(dest, indent, assignable),
        TypeKind::Union(ty, rest) => {
            write_type(dest, indent, ty)?;
            write!(dest, " | ")?;
            write_type(dest, indent, rest)
        }
        TypeKind::Fn(params, ret) => {
            write!(dest, "fn")?;
            if !params.is_empty() {
                write!(dest, " ")?;
                write_types(dest, indent, &params.iter().collect::<Vec<_>>())?;
            }
            write!(dest, " -> ")?;
            write_type(dest, indent, ret)
        }
        TypeKind::Tuple(types) => {
            write!(dest, "(")?;
            if types.is_empty() {
                write!(dest, ",")?;
            } else {
                write_types(dest, indent, &types.iter().collect::<Vec<_>>())?;
            }
            write!(dest, ")")
        }
        TypeKind::List(ty) => {
            write!(dest, "[")?;
            write_type(dest, indent, ty)?;
            write!(dest, "]")
        }
        TypeKind::Set(ty) => {
            write!(dest, "{{")?;
            write_type(dest, indent, ty)?;
            write!(dest, "}}")
        }
        TypeKind::Dict(key, val) => {
            write!(dest, "{{")?;
            write_type(dest, indent, key)?;
            write!(dest, ": ")?;
            write_type(dest, indent, val)?;
            write!(dest, "}}")
        }
        TypeKind::Generic(ident) => write_identifier(dest, ident),
    }
}

fn write_types<W: Write>(dest: &mut W, indent: u32, types: &[&Type]) -> fmt::Result {
    write_comma_separated!(dest, indent, write_type, types);
    Ok(())
}

fn write_assignable<W: Write>(dest: &mut W, indent: u32, assignable: &Assignable) -> fmt::Result {
    match &assignable.kind {
        AssignableKind::Read(identifier) => write_identifier(dest, identifier),
        AssignableKind::Call(callable, args) => {
            write_assignable(dest, indent, callable)?;
            write!(dest, "(")?;
            write_comma_separated!(dest, indent, write_expression, args);
            write!(dest, ")")
        }
        AssignableKind::ArrowCall(first, callable, rest) => {
            write_expression(dest, indent, first)?;
            write!(dest, " -> ")?;
            write_assignable(dest, indent, callable)?;
            write!(dest, " ")?;
            write_comma_separated!(dest, indent, write_expression, rest);
            Ok(())
        }
        AssignableKind::Access(accessable, ident) => {
            write_assignable(dest, indent, accessable)?;
            write!(dest, ".")?;
            write_identifier(dest, ident)
        }
        AssignableKind::Index(indexable, index) => {
            write_assignable(dest, indent, indexable)?;
            write!(dest, "[")?;
            write_expression(dest, indent, index)?;
            write!(dest, "]")
        }
        AssignableKind::Expression(expr) => write_expression(dest, indent, expr),
    }
}

macro_rules! expr_binary_op {
    ($dest:expr, $indent:expr, $lhs:expr, $op:literal, $rhs:expr) => {
        write_expression($dest, $indent, $lhs)?;
        write!($dest, $op)?;
        write_expression($dest, $indent, $rhs)?;
    };
}

fn write_expression<W: Write>(dest: &mut W, indent: u32, expression: &Expression) -> fmt::Result {
    match &expression.kind {
        ExpressionKind::Get(assignable) => write_assignable(dest, indent, assignable)?,
        ExpressionKind::TypeConstant(ty) => {
            write!(dest, ":")?;
            write_type(dest, indent, ty)?;
        }
        ExpressionKind::Add(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " + ", rhs);
        }
        ExpressionKind::Sub(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " - ", rhs);
        }
        ExpressionKind::Mul(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " * ", rhs);
        }
        ExpressionKind::Div(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " / ", rhs);
        }
        ExpressionKind::Neg(expr) => {
            write!(dest, "-")?;
            write_expression(dest, indent, expr)?;
        }
        ExpressionKind::Is(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " is ", rhs);
        }
        ExpressionKind::Eq(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " == ", rhs);
        }
        ExpressionKind::Neq(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " != ", rhs);
        }
        ExpressionKind::Gt(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " > ", rhs);
        }
        ExpressionKind::Gteq(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " >= ", rhs);
        }
        ExpressionKind::Lt(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " < ", rhs);
        }
        ExpressionKind::Lteq(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " <= ", rhs);
        }
        ExpressionKind::AssertEq(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " <=> ", rhs);
        }
        ExpressionKind::In(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " in ", rhs);
        }
        ExpressionKind::And(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " && ", rhs);
        }
        ExpressionKind::Or(lhs, rhs) => {
            expr_binary_op!(dest, indent, lhs, " || ", rhs);
        }
        ExpressionKind::Not(expr) => {
            write!(dest, "!")?;
            write_expression(dest, indent, expr)?;
        }
        ExpressionKind::IfExpression {
            condition,
            pass,
            fail,
        } => {
            write_expression(dest, indent, pass)?;
            write!(dest, " if ")?;
            write_expression(dest, indent, condition)?;
            write!(dest, " else ")?;
            write_expression(dest, indent, fail)?;
        }
        ExpressionKind::Duplicate(expr) => write_expression(dest, indent, expr)?,
        ExpressionKind::IfShort {
            condition,
            fail,
            lhs: _,
        } => {
            write!(dest, "if ")?;
            write_expression(dest, indent, condition)?;
            write!(dest, " else ")?;
            write_expression(dest, indent, fail)?;
        }
        ExpressionKind::Function {
            name: _,
            params,
            ret,
            body,
        } => {
            write!(dest, "fn")?;
            if !params.is_empty() {
                write!(dest, " ")?;
            }
            write_parameters(dest, indent, params)?;
            if matches!(ret.kind, TypeKind::Resolved(RuntimeType::Void)) {
                write!(dest, " ")?;
            } else {
                write!(dest, " -> ")?;
                write_type(dest, indent, ret)?;
                write!(dest, " ")?;
            }
            write_statement(dest, indent, body)?;
        }
        ExpressionKind::Instance { blob, fields } => {
            write_assignable(dest, indent, blob)?;
            write!(dest, " {{\n")?;
            write_blob_instance_fields(dest, indent + 1, fields)?;
            write_indents(dest, indent)?;
            write!(dest, "}}")?;
        }
        ExpressionKind::Tuple(exprs) => {
            write!(dest, "(")?;
            if exprs.is_empty() {
                write!(dest, ",")?;
            } else {
                write_comma_separated!(dest, indent, write_expression, exprs);
            }
            write!(dest, ")")?;
        }
        ExpressionKind::List(exprs) => {
            write!(dest, "[")?;
            write_comma_separated!(dest, indent, write_expression, exprs);
            write!(dest, "]")?;
        }
        ExpressionKind::Set(exprs) => {
            write!(dest, "{{")?;
            write_comma_separated!(dest, indent, write_expression, exprs);
            write!(dest, "}}")?;
        }
        ExpressionKind::Dict(exprs) => {
            write!(dest, "{{")?;
            if exprs.is_empty() {
                write!(dest, ":")?;
            } else {
                let mut first = true;
                let mut exprs = exprs.iter();
                while let Some(expr) = exprs.next() {
                    if !first {
                        write!(dest, ", ")?;
                    }
                    first = false;
                    write_expression(dest, indent, expr)?;
                    write!(dest, ": ")?;
                    write_expression(dest, indent, exprs.next().unwrap())?;
                }
            }
            write!(dest, "}}")?;
        }
        ExpressionKind::Float(f) => write!(dest, "{}", f)?,
        ExpressionKind::Int(i) => write!(dest, "{}", i)?,
        ExpressionKind::Str(s) => write!(dest, "\"{}\"", s)?,
        ExpressionKind::Bool(b) => write!(dest, "{}", if *b { "true" } else { "false" })?,
        ExpressionKind::Nil => write!(dest, "nil")?,
    }

    Ok(())
}

fn write_statement<W: Write>(dest: &mut W, indent: u32, statement: &Statement) -> fmt::Result {
    for comment in &statement.comments {
        write!(dest, "// {}\n", comment)?;
        write_indents(dest, indent)?;
    }

    match &statement.kind {
        StatementKind::Assignment {
            kind,
            target,
            value,
        } => {
            write_assignable(dest, indent, target)?;
            write!(
                dest,
                " {}= ",
                match kind {
                    Op::Nop => "",
                    Op::Add => "+",
                    Op::Sub => "-",
                    Op::Mul => "*",
                    Op::Div => "/",
                }
            )?;
            write_expression(dest, indent, value)?;
        }
        StatementKind::Blob { name, fields } => {
            write!(dest, "{} :: blob {{\n", name)?;
            for (field, ty) in fields {
                write_indents(dest, indent + 1)?;
                write!(dest, "{}: ", field)?;
                write_type(dest, indent, ty)?;
                write!(dest, ",\n")?;
            }
            write_indents(dest, indent)?;
            write!(dest, "}}")?;
        }
        StatementKind::Block { statements } => {
            write!(dest, "{{\n")?;

            for s in &merge_empty_statements(statements.clone()) {
                write_indents(dest, indent + 1)?;
                write_statement(dest, indent + 1, s)?;
                write!(dest, "\n")?;
            }

            write_indents(dest, indent)?;
            write!(dest, "}}")?;
        }
        StatementKind::Break => write!(dest, "break")?,
        StatementKind::Continue => write!(dest, "continue")?,
        StatementKind::Definition {
            ident,
            kind,
            ty,
            value,
        } => {
            write_identifier(dest, ident)?;
            if matches!(ty.kind, TypeKind::Implied) {
                write!(
                    dest,
                    "{}",
                    match kind {
                        VarKind::Const => " :: ",
                        VarKind::Mutable => " := ",
                        VarKind::ForceConst => unreachable!("can't force an implied type"),
                        VarKind::ForceMutable => unreachable!("can't force an implied type"),
                    }
                )?;
            } else {
                write!(dest, ": ")?;
                if kind.force() {
                    write!(dest, "!")?;
                }
                write_type(dest, indent, ty)?;
                if kind.immutable() {
                    write!(dest, " : ")?;
                } else {
                    write!(dest, " = ")?;
                }
            }
            write_expression(dest, indent, value)?;
        }
        StatementKind::EmptyStatement => (),
        StatementKind::If {
            condition,
            pass,
            fail,
        } => {
            if matches!(fail.kind, StatementKind::EmptyStatement) {
                for comment in &fail.comments {
                    write!(dest, "// {}\n", comment)?;
                    write_indents(dest, indent)?;
                }
            }

            write!(dest, "if ")?;
            write_expression(dest, indent, condition)?;
            write!(dest, " ")?;
            write_statement(dest, indent, pass)?;
            if !matches!(fail.kind, StatementKind::EmptyStatement) {
                write!(dest, " else ")?;
                write_statement(dest, indent, fail)?;
            }
        }
        StatementKind::IsCheck { lhs, rhs } => {
            write_type(dest, indent, lhs)?;
            write!(dest, " is ")?;
            write_type(dest, indent, rhs)?;
        }
        StatementKind::Loop { condition, body } => {
            write!(dest, "loop ")?;
            write_expression(dest, indent, condition)?;
            write!(dest, " ")?;
            write_statement(dest, indent, body)?;
        }
        StatementKind::Ret { value } => {
            write!(dest, "ret ")?;
            write_expression(dest, indent, value)?;
        }
        StatementKind::StatementExpression { value } => write_expression(dest, indent, value)?,
        StatementKind::Unreachable => {
            write!(dest, "<!>")?;
        }
        StatementKind::Use {
            path,
            name,
            file: _,
        } => {
            write!(dest, "use ")?;
            write_identifier(dest, path)?;
            if let NameIdentifier::Alias(alias) = name {
                write!(dest, " as ")?;
                write_identifier(dest, alias)?;
            }
        }
    }

    Ok(())
}

/// Replace consecutive empty statements with one empty statement with all comments of the previous statements.
//TODO(gu): Rewrite the formatter to use moves instead of borrows. Then we wouldn't need to clone when passing
//          into this function.
fn merge_empty_statements(mut statements: Vec<Statement>) -> Vec<Statement> {
    // Reverse since
    // - we always want to remove and look at the first statement and
    // - pop() is faster than remove(0).
    statements.reverse();

    let mut ret = Vec::new();
    while let Some(mut statement) = statements.pop() {
        if !matches!(statement.kind, StatementKind::EmptyStatement) {
            ret.push(statement);
            continue;
        }
        // Begin eating empty statements
        while matches!(
            statements.last().map(|s| &s.kind),
            Some(StatementKind::EmptyStatement)
        ) {
            statement
                .comments
                .append(&mut statements.pop().unwrap().comments);
        }
        ret.push(statement);
    }
    ret
}

fn format_module(module: &Module) -> Result<String, fmt::Error> {
    let mut formatted = String::new();
    merge_empty_statements(module.statements.clone())
        .iter()
        // Side effects incoming!
        .map(|s| {
            write_statement(&mut formatted, 0, s)?;
            write!(formatted, "\n")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(formatted)
}

pub fn format(args: &Args) -> Result<String, Vec<Error>> {
    let tree = sylt_parser::tree(&PathBuf::from(args.args.first().expect("No file to run")))?;
    Ok(format_module(&tree.modules[0].1).unwrap())
}

#[cfg(test)]
mod tests {
    macro_rules! test_formatter_on_file {
        ($fn:ident, $path:literal, $print:expr, $errs:pat) => {
            #[test]
            fn $fn() {
                #[allow(unused_imports)]
                use ::sylt_common::{error::{Error, RuntimeError, TypeError}, Type};
                #[allow(unused_imports)]
                use ::sylt_tokenizer::Span;

                let path = format!("../{}", $path);

                // Run the file before the formatter.
                let mut args = $crate::Args::default();
                args.args = vec![path.clone()];
                let before = $crate::run_file(&args, ::sylt_std::sylt::_sylt_link());
                // If the test fails here, we already have / will have prettified output.
                assert!(
                    matches!(before.err().unwrap_or(Vec::new()).as_slice(), $errs),
                    "the test failed before the formatter was called"
                );

                // We now know that before contains $errs exactly.

                // Format the file.
                match $crate::formatter::format(&args) {
                    Ok(formatted) => {
                        // Overwrite with the formatted output.
                        ::std::fs::write(&path, formatted).unwrap();

                        // Try to run the file again, this time with pretty "got/expected"-output.
                        let after = $crate::run_file(&args, ::sylt_std::sylt::_sylt_link());
                        eprintln!("If you can read this, the test output changed between before and after formatting");
                        $crate::assert_errs!(after, $errs);
                    }
                    Err(errs) => {
                        eprintln!("If you can read this, the formatter couldn't parse the file but the syntax errors");
                        eprintln!("changed between before and after formatting.");
                        let errs: Result<(), _> = Err(errs); //TODO(gu): Result<!, _> ;)
                        $crate::assert_errs!(errs, $errs);
                    }
                }
            }
        }
    }

    sylt_macro::find_tests!(test_formatter_on_file);
}
