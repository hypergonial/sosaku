use std::borrow::Cow;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{escaped, escaped_transform, is_not, tag, take_while},
    character::complete::{anychar, char, digit1, multispace0, one_of},
    combinator::{map, map_res, opt, recognize, value},
    error::ParseError,
    multi::{separated_list0, separated_list1},
    sequence::{delimited, preceded, separated_pair, terminated},
};

use super::types::{Exp, FunctionItem, Value, VarAccess, VarName};

static KEYWORDS: [&str; 3] = ["true", "false", "null"];

// TODO: Consider migrating to winnow?
// https://docs.rs/winnow/latest/winnow/index.html

struct BinaryOperator<'a> {
    op: &'static str,
    func: fn(Exp<'a>, Exp<'a>) -> Exp<'a>,
}

impl<'a> BinaryOperator<'a> {
    const fn new(op: &'static str, func: fn(Exp<'a>, Exp<'a>) -> Exp<'a>) -> Self {
        Self { op, func }
    }
}

/// Remove trailing whitespaces from the inner parser
fn ws<'a, F, O, E>(inner: F) -> impl Parser<&'a str, Output = O, Error = E>
where
    F: FnMut(&'a str) -> IResult<&'a str, O, E> + 'a,
    E: ParseError<&'a str>,
{
    terminated(inner, multispace0)
}

/// Parse an integer
fn integer(input: &str) -> IResult<&str, i64> {
    map_res(recognize(opt(one_of("+-")).and(digit1)), |o: &str| {
        o.parse::<i64>()
    })
    .parse(input)
}

/// Parse a floating point number
fn double(input: &str) -> IResult<&str, f64> {
    map_res(
        recognize(opt(one_of("+-")).and(digit1).and(tag(".")).and(digit1)),
        |o: &str| o.parse::<f64>(),
    )
    .parse(input)
}

fn raw_string(input: &str) -> IResult<&str, Cow<'_, str>> {
    alt((
        map(
            preceded(
                char('r'),
                delimited(
                    char('\''),
                    opt(escaped(is_not("\\'"), '\\', anychar)),
                    char('\''),
                ),
            ),
            |s: Option<&str>| Cow::Borrowed(s.unwrap_or("")),
        ),
        map(
            preceded(
                char('r'),
                delimited(
                    char('"'),
                    opt(escaped(is_not("\\\""), '\\', anychar)),
                    char('"'),
                ),
            ),
            |s: Option<&str>| Cow::Borrowed(s.unwrap_or("")),
        ),
    ))
    .parse(input)
}

fn cooked_string(input: &str) -> IResult<&str, Cow<'_, str>> {
    let single = delimited(
        char('\''),
        opt(escaped_transform(
            is_not("\\'"),
            '\\',
            alt((
                value("\\", char('\\')),
                value("'", char('\'')),
                value("\n", char('n')),
                value("\r", char('r')),
                value("\t", char('t')),
                value("\0", char('0')),
            )),
        )),
        char('\''),
    );

    let double = delimited(
        char('"'),
        opt(escaped_transform(
            is_not("\\\""),
            '\\',
            alt((
                value("\\", char('\\')),
                value("\"", char('"')),
                value("\n", char('n')),
                value("\r", char('r')),
                value("\t", char('t')),
                value("\0", char('0')),
            )),
        )),
        char('"'),
    );

    alt((
        map(single, |v| v.map_or(Cow::Borrowed(""), Cow::Owned)),
        map(double, |v| v.map_or(Cow::Borrowed(""), Cow::Owned)),
    ))
    .parse(input)
}

/// Parse a quoted string, handling both single and double quotes, as well as escaped characters,
/// with an optional "r" prefix for raw strings
fn string(input: &str) -> IResult<&str, Cow<'_, str>> {
    alt((raw_string, cooked_string)).parse(input)
}

/// Parse an array literal (e.g. `[1, 2, 3]`)
///
/// ## Parameters
///
/// - `input`: The input string to parse
///
/// ## Returns
///
/// The parsed array as a vector of expressions
///
/// ## Errors
///
/// - If the input string does not match the expected pattern
///   (e.g. missing brackets, missing commas, etc.), a parsing error will be returned.
fn parse_array(input: &str) -> IResult<&str, Vec<Exp<'_>>> {
    delimited(
        ws(char('[')),
        separated_list0(ws(char(',')), parse_exp),
        ws(char(']')),
    )
    .parse(input)
}

/// Parse an object literal (e.g. `{"key": "value", "foo": 123}`)
///
/// ## Parameters
///
/// - `input`: The input string to parse
///
/// ## Returns
///
/// The parsed object as a vector of key-value pairs, where the key is a string and the value is an expression
fn parse_object(input: &str) -> IResult<&str, Vec<(String, Exp<'_>)>> {
    delimited(
        ws(char('{')),
        separated_list0(
            ws(char(',')),
            separated_pair(map(string, Cow::into_owned), ws(char(':')), parse_exp),
        ),
        ws(char('}')),
    )
    .parse(input)
}

/// Parse a non-keyword identifier
fn parse_non_keyword(input: &str) -> IResult<&str, &str> {
    map_res(
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
        |v: &str| {
            if v.is_empty() {
                Err("Parsed empty string")
            } else if KEYWORDS.contains(&v) {
                Err("Parsed a keyword")
            } else {
                Ok(v)
            }
        },
    )
    .parse(input)
}

// Parse a variable name: A variable name is a series of non-keywords separated by dots, with an optional indexer at the end (e.g. "foo.bar[0]")
pub(super) fn parse_variable_name(input: &str) -> IResult<&str, VarAccess> {
    let (input, names) = separated_list1(
        char('.'),
        alt((parse_non_keyword.map(Cow::Borrowed), string)).and(opt(delimited(
            ws(char('[')),
            ws(digit1),
            ws(char(']')),
        ))),
    )
    .parse(input)?;

    // If any of the names start with a digit, it's an error (e.g. "foo.0bar")
    for (name, _) in &names {
        if name
            .chars()
            .next()
            .expect("Variable name is empty, should have been caught by parse_non_keyword")
            .is_ascii_digit()
        {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Digit,
            )));
        }
    }

    let varaccess = VarAccess::new(
        names
            .into_iter()
            .map(|(name, index)| {
                VarName::new(
                    name,
                    index.map(|i| i.parse::<usize>().expect("Failed to parse index")),
                )
            })
            .collect::<Vec<_>>(),
    );

    Ok((input, varaccess))
}

// Function that tries all ops & returns the remaining input & the op that worked (if any)
fn try_ops<'a, 'b, 'c>(
    ops: &'b [BinaryOperator<'c>],
    input: &'a str,
) -> IResult<&'a str, &'b BinaryOperator<'c>> {
    for op in ops {
        let parsed: IResult<&str, &str> = ws(tag(op.op)).parse(input);
        if let Ok((remainder, _)) = parsed {
            return Ok((remainder, op));
        }
    }
    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

/// Parse a left-associative binary operator
///
/// ## Parameters
///
/// - `parser`: The parser for the individual operands
/// - `ops`: The operators to apply
/// - `input`: The input string
///
/// ## Returns
///
/// The parsed expression
fn parse_left_assoc<'a, 'b, E: ParseError<&'a str>>(
    mut parser: impl Parser<&'a str, Output = Exp<'a>, Error = E>,
    ops: &'b [BinaryOperator<'a>],
    input: &'a str,
) -> IResult<&'a str, Exp<'a>, E> {
    let (mut input, mut current) = parser.parse(input)?;

    loop {
        // Break out of the loop if no operator was matched
        let Ok((i, op)) = try_ops(ops, input) else {
            return Ok((input, current));
        };
        // RHS should always exist
        let (i, rhs) = parser.parse(i)?;
        current = (op.func)(current, rhs);
        input = i;
    }
}

/// Parse a right-associative binary operator
///
/// ## Parameters
///
/// - `parser`: The parser for the individual operands
/// - `ops`: The operators to apply
/// - `input`: The input string
///
/// ## Returns
///
/// The parsed expression
#[expect(dead_code)]
fn parse_right_assoc<'a, 'b, E: ParseError<&'a str>>(
    mut parser: impl Parser<&'a str, Output = Exp<'a>, Error = E>,
    ops: &'b [BinaryOperator<'a>],
    input: &'a str,
) -> IResult<&'a str, Exp<'a>, E> {
    let (mut input, mut current) = parser.parse(input)?;

    let mut stack = Vec::new();

    while let Ok((i, op)) = try_ops(ops, input) {
        // RHS should always exist
        let (i, rhs) = parser.parse(i)?;
        stack.push((op, current));
        current = rhs;
        input = i;
    }

    while let Some((op, lhs)) = stack.pop() {
        current = (op.func)(lhs, current);
    }

    Ok((input, current))
}

/// Parse a non-associative binary operator
///
/// ## Parameters
///
/// - `parser`: The parser for the individual operands
/// - `ops`: The operators to apply
/// - `input`: The input string
///
/// ## Returns
///
/// The parsed expression
///
/// ## Errors
///
/// - If the number of operands is not 1 or 2
/// - If the parser fails
fn parse_non_assoc<'a, 'b, E: ParseError<&'a str> + 'a>(
    parser: impl Parser<&'a str, Output = Exp<'a>, Error = E>,
    op: &'b BinaryOperator<'a>,
    input: &'a str,
) -> IResult<&'a str, Exp<'a>, E> {
    let (input, mut exprs) = separated_list1(ws(tag(op.op)), parser).parse(input)?;

    let proc = match exprs.len() {
        1 => Ok(exprs.pop().expect("Impossible")),
        2 => Ok((op.func)(exprs.remove(0), exprs.remove(0))),
        _ => Err(nom::Err::Error(E::from_error_kind(
            input,
            nom::error::ErrorKind::Count,
        )))?,
    }?;

    Ok((input, proc))
}

/// Parse a matcher function from the input string and return a `ParserFunction` struct
///
/// ## Parameters
/// - `input`: The input string to parse, e.g. "startsWith('hello')"
///
/// ## Returns
/// - `Ok(ParserFunction)`: If the parsing is successful, returns a `ParserFunction` struct containing the function name and value
///
/// ## Errors
///
/// - If the input string does not match the expected pattern (e.g. missing parentheses, missing quotes, etc.), a parsing error will be returned.
fn parse_fn(input: &str) -> IResult<&str, FunctionItem<'_>> {
    let (input, (name, _, _, _, value, _, _)) = (
        map_res(take_while(|c: char| c.is_alphabetic()), |v: &str| {
            if v.is_empty() {
                Err("Empty function name")
            } else {
                Ok(v)
            }
        }), // Function name
        take_while(|c: char| c.is_whitespace()),
        char('('),
        take_while(|c: char| c.is_whitespace()),
        // The parameter list, comma-separated, with an optional trailing comma at the end
        separated_list0(ws(char(',')), parse_exp)
            .and(opt(ws(char(','))))
            .map(|(list, _)| list),
        take_while(|c: char| c.is_whitespace()),
        char(')'),
    )
        .parse(input)?;

    Ok((input, FunctionItem::new(name, value)))
}

/// Parse a literal value (integer, float, boolean, or string)
fn parse_literal(input: &str) -> IResult<&str, Value<'_>> {
    alt((
        ws(double).map(Value::Float),
        ws(integer).map(Value::Int),
        alt((ws(tag("true")), ws(tag("false")))).map(|v: &str| Value::Bool(v == "true")),
        ws(tag("null")).map(|_| Value::Null),
        ws(string).map(Value::String),
    ))
    .parse(input)
}

/// Parse an atomic expression
fn parse_atom(input: &str) -> IResult<&str, Exp<'_>> {
    alt((
        ws(parse_literal).map(Exp::literal),
        ws(parse_array).map(Exp::array),
        ws(parse_object).map(|v| Exp::object(v.into_iter().collect())),
        // Function call
        ws(parse_fn).map(Exp::fn_call),
        // Variable names
        ws(parse_variable_name).map(|v: VarAccess| Exp::Var(v)),
        // Parenthesized expressions
        delimited(ws(char('(')), parse_exp, ws(char(')'))),
    ))
    .parse(input)
}

/// Parse a negation (prefix unary !) operator
fn parse_neg(input: &str) -> IResult<&str, Exp<'_>> {
    // Try reading a negation operator
    let Ok((input, _)): IResult<&str, &str> = ws(tag("!")).parse(input) else {
        return parse_atom(input);
    };
    // If successful, wrap the resulting expression in a negation
    let (input, exp) = parse_atom(input)?;
    Ok((input, Exp::neg(exp)))
}

fn parse_comp(input: &str) -> IResult<&str, Exp<'_>> {
    parse_left_assoc(
        parse_neg,
        &[
            BinaryOperator::new(">=", Exp::geq),
            BinaryOperator::new("<=", Exp::leq),
            BinaryOperator::new(">", Exp::gt),
            BinaryOperator::new("<", Exp::lt),
        ],
        input,
    )
}

fn parse_neq(input: &str) -> IResult<&str, Exp<'_>> {
    parse_non_assoc(parse_comp, &BinaryOperator::new("!=", Exp::neq), input)
}

fn parse_eq(input: &str) -> IResult<&str, Exp<'_>> {
    parse_non_assoc(parse_neq, &BinaryOperator::new("==", Exp::eq), input)
}

fn parse_and(input: &str) -> IResult<&str, Exp<'_>> {
    parse_left_assoc(parse_eq, &[BinaryOperator::new("&&", Exp::and)], input)
}

fn parse_or(input: &str) -> IResult<&str, Exp<'_>> {
    parse_left_assoc(parse_and, &[BinaryOperator::new("||", Exp::or)], input)
}

pub(super) fn parse_exp(input: &str) -> IResult<&str, Exp<'_>> {
    parse_or(input.trim_start())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_atom() {
        assert_eq!(parse_atom("123"), Ok(("", Exp::literal(Value::Int(123)))));
        assert_eq!(
            parse_atom("-123  "),
            Ok(("", Exp::literal(Value::Int(-123))))
        );
        assert_eq!(
            parse_atom("123.456  "),
            Ok(("", Exp::literal(Value::Float(123.456))))
        );
        assert_eq!(
            parse_atom("-123.456  "),
            Ok(("", Exp::literal(Value::Float(-123.456))))
        );
        assert_eq!(
            parse_atom("true  "),
            Ok(("", Exp::literal(Value::Bool(true))))
        );
        assert_eq!(
            parse_atom("false  "),
            Ok(("", Exp::literal(Value::Bool(false))))
        );
        assert_eq!(parse_atom("abc"), Ok(("", Exp::varname("abc").unwrap())));

        assert_eq!(
            parse_atom("'hello\\' world'    "),
            Ok(("", Exp::literal(Value::String("hello' world".into()))))
        );

        assert_eq!(parse_atom("null "), Ok(("", Exp::literal(Value::Null))));
    }

    #[test]
    fn test_var_access() {
        assert_eq!(
            parse_variable_name("foo.bar[ 0 ].baz"),
            Ok((
                "",
                VarAccess::new(vec![
                    VarName::new("foo", None),
                    VarName::new("bar", Some(0)),
                    VarName::new("baz", None),
                ])
            ))
        );
        // Variable names with digits should work as long as they don't start with a digit
        assert_eq!(
            parse_variable_name("foo1.bar2[3].baz4"),
            Ok((
                "",
                VarAccess::new(vec![
                    VarName::new("foo1", None),
                    VarName::new("bar2", Some(3)),
                    VarName::new("baz4", None),
                ])
            ))
        );
        // Variable names that start with a digit should fail
        assert!(parse_variable_name("foo.0bar").is_err());
    }

    #[test]
    fn test_neg() {
        assert_eq!(
            parse_neg("!123"),
            Ok(("", Exp::neg(Exp::literal(Value::Int(123)))))
        );
        assert_eq!(
            parse_neg("!true"),
            Ok(("", Exp::neg(Exp::literal(Value::Bool(true)))))
        );
        assert_eq!(
            parse_neg("!false"),
            Ok(("", Exp::neg(Exp::literal(Value::Bool(false)))))
        );
        assert_eq!(
            parse_neg("!abc"),
            Ok(("", Exp::neg(Exp::varname("abc").unwrap())))
        );
    }

    #[test]
    fn test_comp() {
        assert_eq!(
            parse_comp("123 > 456"),
            Ok((
                "",
                Exp::gt(Exp::literal(Value::Int(123)), Exp::literal(Value::Int(456)))
            ))
        );
        assert_eq!(
            parse_comp("123 < 456"),
            Ok((
                "",
                Exp::lt(Exp::literal(Value::Int(123)), Exp::literal(Value::Int(456)))
            ))
        );
        assert_eq!(
            parse_comp("123 >= 456"),
            Ok((
                "",
                Exp::geq(Exp::literal(Value::Int(123)), Exp::literal(Value::Int(456)))
            ))
        );
        assert_eq!(
            parse_comp("123 <= 456"),
            Ok((
                "",
                Exp::leq(Exp::literal(Value::Int(123)), Exp::literal(Value::Int(456)))
            ))
        );
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(
            parse_exp("\n\n   \r\n123   "),
            Ok(("", Exp::literal(Value::Int(123))))
        );
        assert_eq!(
            parse_exp("\n\n   \r\n!   123   \n\n   "),
            Ok(("", Exp::neg(Exp::literal(Value::Int(123)))))
        );
        assert_eq!(
            parse_exp("\n\n   \r\n123   >   456   \n\n   "),
            Ok((
                "",
                Exp::gt(Exp::literal(Value::Int(123)), Exp::literal(Value::Int(456)))
            ))
        );
        assert_eq!(
            parse_exp("\n     len(y) == 5"),
            Ok((
                "",
                Exp::eq(
                    Exp::fn_call(FunctionItem::new("len", vec![Exp::varname("y").unwrap()])),
                    Exp::literal(Value::Int(5))
                )
            ))
        );
    }

    #[test]
    fn test_eq() {
        assert_eq!(
            parse_exp("1 == 2"),
            Ok((
                "",
                Exp::eq(Exp::literal(Value::Int(1)), Exp::literal(Value::Int(2)))
            ))
        );
        assert!(parse_exp("1 == 2 == 3").is_err());
    }

    #[test]
    fn test_neq() {
        assert_eq!(
            parse_exp("1 != 2"),
            Ok((
                "",
                Exp::neq(Exp::literal(Value::Int(1)), Exp::literal(Value::Int(2)))
            ))
        );
        assert!(parse_exp("1 != 2 != 3").is_err());
    }

    #[test]
    fn test_and() {
        assert_eq!(
            parse_exp("true && false"),
            Ok((
                "",
                Exp::and(
                    Exp::literal(Value::Bool(true)),
                    Exp::literal(Value::Bool(false))
                )
            ))
        );
        assert_eq!(
            parse_exp("true && false && true"),
            Ok((
                "",
                Exp::and(
                    Exp::and(
                        Exp::literal(Value::Bool(true)),
                        Exp::literal(Value::Bool(false))
                    ),
                    Exp::literal(Value::Bool(true))
                )
            ))
        );
    }

    #[test]
    fn test_and_or_precedence() {
        assert_eq!(
            parse_exp("true || false && false"),
            Ok((
                "",
                Exp::or(
                    Exp::literal(Value::Bool(true)),
                    Exp::and(
                        Exp::literal(Value::Bool(false)),
                        Exp::literal(Value::Bool(false))
                    )
                )
            ))
        );
    }

    #[test]
    fn test_or() {
        assert_eq!(
            parse_exp("true || false"),
            Ok((
                "",
                Exp::or(
                    Exp::literal(Value::Bool(true)),
                    Exp::literal(Value::Bool(false))
                )
            ))
        );
        assert_eq!(
            parse_exp("true || false || true"),
            Ok((
                "",
                Exp::or(
                    Exp::or(
                        Exp::literal(Value::Bool(true)),
                        Exp::literal(Value::Bool(false))
                    ),
                    Exp::literal(Value::Bool(true))
                )
            ))
        );
    }

    #[test]
    fn test_parens() {
        assert_eq!(
            parse_exp("(true || false) && true"),
            Ok((
                "",
                Exp::and(
                    Exp::or(
                        Exp::literal(Value::Bool(true)),
                        Exp::literal(Value::Bool(false))
                    ),
                    Exp::literal(Value::Bool(true))
                )
            ))
        );
    }

    #[test]
    fn test_variable_names() {
        assert_eq!(parse_exp("foo"), Ok(("", Exp::varname("foo").unwrap())));

        assert_eq!(
            parse_exp("foo_bar"),
            Ok(("", Exp::varname("foo_bar").unwrap()))
        );

        assert_eq!(
            parse_exp("foo||bar"),
            Ok((
                "",
                Exp::or(Exp::varname("foo").unwrap(), Exp::varname("bar").unwrap())
            ))
        );

        assert_eq!(
            parse_exp("foo    && bar"),
            Ok((
                "",
                Exp::and(Exp::varname("foo").unwrap(), Exp::varname("bar").unwrap())
            ))
        );
    }

    #[test]
    fn test_escaped_variable_names() {
        assert_eq!(
            parse_exp("foo.'bar/baz'[1].qux[0]"),
            Ok((
                "",
                Exp::Var(VarAccess::new(vec![
                    VarName::new("foo", None),
                    VarName::new("bar/baz", Some(1)),
                    VarName::new("qux", Some(0)),
                ]))
            ))
        );
    }

    #[test]
    fn test_crazy_nested() {
        assert_eq!(
            parse_exp("!(1 > 2) && (3 <= 4 || 5 != 6)"),
            Ok((
                "",
                Exp::and(
                    Exp::neg(Exp::gt(
                        Exp::literal(Value::Int(1)),
                        Exp::literal(Value::Int(2))
                    )),
                    Exp::or(
                        Exp::leq(Exp::literal(Value::Int(3)), Exp::literal(Value::Int(4))),
                        Exp::neq(Exp::literal(Value::Int(5)), Exp::literal(Value::Int(6)))
                    )
                )
            ))
        );
    }

    #[test]
    fn test_crazy_nested_fn() {
        assert_eq!(
            parse_exp("!startsWith('hello') && (3 <= 4 || 5 != 6)"),
            Ok((
                "",
                Exp::and(
                    Exp::neg(Exp::fn_call(FunctionItem::new(
                        "startsWith",
                        vec![Exp::literal(Value::String("hello".into()))]
                    ))),
                    Exp::or(
                        Exp::leq(Exp::literal(Value::Int(3)), Exp::literal(Value::Int(4))),
                        Exp::neq(Exp::literal(Value::Int(5)), Exp::literal(Value::Int(6)))
                    )
                )
            ))
        );
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(
            parse_literal("'hello world'"),
            Ok(("", Value::String("hello world".into())))
        );
        assert_eq!(
            parse_literal("\"hello world\""),
            Ok(("", Value::String("hello world".into())))
        );
        assert_eq!(
            parse_literal("'hello \\'world\\''"),
            Ok(("", Value::String("hello 'world'".into())))
        );
        assert_eq!(
            parse_literal("\"hello \\\"world\\\"\""),
            Ok(("", Value::String("hello \"world\"".into())))
        );
        assert_eq!(
            parse_literal("'hello\\nworld'"),
            Ok(("", Value::String("hello\nworld".into())))
        );
        assert_eq!(
            parse_literal("'hello\\tworld'"),
            Ok(("", Value::String("hello\tworld".into())))
        );
        assert_eq!(
            parse_literal("r'hello\\n\\'world'"),
            Ok(("", Value::String("hello\\n\\'world".into())))
        );
        assert_eq!(
            parse_literal("r\"hello\\n\\\"world\""),
            Ok(("", Value::String("hello\\n\\\"world".into())))
        );
    }

    #[test]
    fn test_array_parser() {
        let (_, array) = parse_array("[1, 2, 3]").expect("Failed to parse array");

        assert_eq!(
            array,
            vec![
                Exp::literal(Value::Int(1)),
                Exp::literal(Value::Int(2)),
                Exp::literal(Value::Int(3))
            ]
        );
    }

    #[test]
    fn test_heterogeneous_array_parser() {
        let (_, array) = parse_array("[1, 'two', 3.0]").expect("Failed to parse array");

        assert_eq!(
            array,
            vec![
                Exp::literal(Value::Int(1)),
                Exp::literal(Value::String("two".into())),
                Exp::literal(Value::Float(3.0))
            ]
        );
    }

    #[test]
    fn test_object_parser() {
        let (_, object) =
            parse_object("{\"key\": \"value\", \"foo\": 123}").expect("Failed to parse object");

        assert_eq!(
            object,
            vec![
                ("key".into(), Exp::literal(Value::String("value".into()))),
                ("foo".into(), Exp::literal(Value::Int(123)))
            ]
        );
    }

    #[test]
    fn test_fn_parser() {
        let (_, parser_function) =
            parse_fn("startsWith('hello')").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "startsWith");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::String("hello".into()))]
        );
    }

    #[test]
    fn test_fn_parser_noargs() {
        let (_, parser_function) = parse_fn("isEmpty()").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "isEmpty");
        assert_eq!(parser_function.args(), vec![]);
    }

    #[test]
    fn test_fn_parser_doublequote() {
        let (_, parser_function) =
            parse_fn("startsWith(\"hello\")").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "startsWith");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::String("hello".into()))]
        );
    }

    #[test]
    fn test_fn_parser_multiple_args() {
        let (_, parser_function) =
            parse_fn("between(1, 10)").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "between");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::Int(1)), Exp::literal(Value::Int(10))]
        );
    }

    #[test]
    fn test_fn_parser_trailing_comma() {
        let (_, parser_function) =
            parse_fn("between(1, 10,)").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "between");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::Int(1)), Exp::literal(Value::Int(10))]
        );
    }

    #[test]
    fn test_fn_parser_nested_args() {
        let (_, parser_function) =
            parse_fn("between(length('hello'), 10)").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "between");
        assert_eq!(
            parser_function.args(),
            vec![
                Exp::fn_call(FunctionItem::new(
                    "length",
                    vec![Exp::literal(Value::String("hello".into()))]
                )),
                Exp::literal(Value::Int(10))
            ]
        );
    }

    #[test]
    fn test_fn_parser_regex() {
        // Failed to parse matcher function: Error(Error { input: "\"[^\\.]*test.ya?ml\")", code: Char })
        let (_, parser_function) =
            parse_fn(r#"matches(r"[^\.]*test.ya?ml")"#).expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "matches");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::String(r"[^\.]*test.ya?ml".into()))]
        );
    }

    #[test]
    fn test_whitespace_between_fn_and_parentheses() {
        let (_, parser_function) =
            parse_fn("startsWith   (  'hello'  )").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "startsWith");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::String("hello".into()))]
        );
    }

    #[test]
    fn test_missing_parentheses() {
        let result = parse_fn("startsWith'hello')");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_quotes() {
        let result = parse_fn("startsWith('hello)");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_value() {
        let (_, parser_function) =
            parse_fn("startsWith('')").expect("Failed to parse matcher function");

        assert_eq!(parser_function.name(), "startsWith");
        assert_eq!(
            parser_function.args(),
            vec![Exp::literal(Value::String("".into()))]
        );
    }
}
