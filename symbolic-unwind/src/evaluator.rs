use super::memory::MemoryRegion;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::{Add, Div, Mul, Rem, Sub};

/// Structure that encapsulates the information necessary to evaluate Breakpad
/// RPN expressions:
///
/// - A region of memory
/// - Values of constants
/// - Values of variables
pub struct MemoryEvaluator<M, T> {
    /// A region of memory.
    ///
    /// If this is `None`, evaluation of expressions containing dereference
    /// operations will fail.
    pub memory: Option<M>,

    /// A map containing the values of constants.
    ///
    /// Trying to use a constant that is not in this map will cause evaluation to fail.
    pub constants: HashMap<Constant, T>,

    /// A map containing the values of variables.
    ///
    /// Trying to use a variable that is not in this map will cause evaluation to fail.
    /// This map can be modified by the [`assign`](Self::assign) and
    ///  [`process`](Self::process) methods.
    pub variables: HashMap<Variable, T>,
}

impl<T, M: MemoryRegion<T>> MemoryEvaluator<M, T>
where
    T: Into<u64>
        + Add<Output = T>
        + Mul<Output = T>
        + Div<Output = T>
        + Sub<Output = T>
        + Rem<Output = T>
        + Copy
        + std::fmt::Debug
{
    /// Evaluates a single expression.
    ///
    /// This may fail if the expression tries to dereference unavailable memory
    /// or uses undefined constants or variables.
    pub fn evaluate(&self, expr: &Expr<T>) -> Result<T, EvaluationError> {
        use Expr::*;
        match expr {
            Value(x) => Ok(*x),
            Const(c) => self
                .constants
                .get(&c)
                .copied()
                .ok_or_else(|| EvaluationError::UndefinedConstant(c.clone())),
            Var(v) => self
                .variables
                .get(&v)
                .copied()
                .ok_or_else(|| EvaluationError::UndefinedVariable(v.clone())),
            Op(e1, e2, op) => {
                let e1 = self.evaluate(&*e1)?;
                let e2 = self.evaluate(&*e2)?;
                match op {
                    BinOp::Add => Ok(e1 + e2),
                    BinOp::Sub => Ok(e1 - e2),
                    BinOp::Mul => Ok(e1 * e2),
                    BinOp::Div => Ok(e1 / e2),
                    BinOp::Mod => Ok(e1 % e2),
                    BinOp::Align => Ok(e2 * (e1 / e2)),
                }
            }
            Deref(address) => {
                if let Some(ref memory) = self.memory {
                    let address = self.evaluate(&*address)?;
                    memory
                        .get(address.into())
                        .ok_or(EvaluationError::MemoryOutOfBounds {
                            address: address.into(),
                            base: memory.base_addr(),
                            size: memory.size(),
                        })
                } else {
                    Err(EvaluationError::MemoryUnavailable)
                }
            }
        }
    }

    /// Performs an assignment by first evaluating its right-hand side and then
    /// modifying [`variables`](Self::variables) accordingly.
    ///
    /// This may fail if the right-hand side cannot be evaluated, cf.
    /// [`evaluate`](Self::evaluate). It returns `true` iff the assignment modified
    /// an existing variable.
    pub fn assign(&mut self, Assignment(v, e): &Assignment<T>) -> Result<bool, EvaluationError> {
        let value = self.evaluate(e)?;
        Ok(self.variables.insert(v.clone(), value).is_some())
    }
}
impl<T: std::fmt::Debug, M: MemoryRegion<T>> MemoryEvaluator<M, T> {
    /// Processes a string of assignments, modifying its [`variables`](Self::variables)
    /// field accordingly.
    ///
    /// This may fail if parsing goes wrong or a parsed assignment cannot be handled,
    /// cf. [`assign`](Self::assign).
    pub fn process<'a>(
        &'a mut self,
        input: &'a str,
    ) -> Result<HashSet<Variable>, ExpressionError<'a>>
    where
        T: Into<u64>
            + Add<Output = T>
            + Mul<Output = T>
            + Div<Output = T>
            + Sub<Output = T>
            + Rem<Output = T>
            + std::str::FromStr
            + Copy
            + std::fmt::Debug
    {
        let mut changed_variables = HashSet::new();
        let assignments = parsing::assignments::<T>(input)?;
        for a in assignments {
            self.assign(&a)?;
            changed_variables.insert(a.0);
        }

        Ok(changed_variables)
    }
}

/// An error encountered while evaluating an expression.
#[derive(Debug)]
pub enum EvaluationError {
    /// The expression contains an undefined constant.
    UndefinedConstant(Constant),
    /// The expression contains an undefined variable.
    UndefinedVariable(Variable),
    /// The expression contains a dereference, but no memory region is available.
    MemoryUnavailable,
    /// The requested piece of memory would exceed the bounds of the memory region.
    MemoryOutOfBounds { address: u64, base: u64, size: u32 },
}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
pub enum ExpressionError<'a> {
    /// An error was encountered while parsing an expression.
    Parsing(parsing::ExprParsingError<'a>),
    /// An error was encountered while evaluating an expression.
    Evaluation(EvaluationError),
}

impl<'a> From<parsing::ExprParsingError<'a>> for ExpressionError<'a> {
    fn from(other: parsing::ExprParsingError<'a>) -> Self {
        Self::Parsing(other)
    }
}

impl<'a> From<EvaluationError> for ExpressionError<'a> {
    fn from(other: EvaluationError) -> Self {
        Self::Evaluation(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Variable(String);

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Constant(String);

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Mod,
    /// Alignment.
    ///
    /// Truncates the first operand to a multiple of the second operand.
    Align,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Add => write!(f, "+"),
            Self::Sub => write!(f, "-"),
            Self::Mul => write!(f, "*"),
            Self::Div => write!(f, "/"),
            Self::Mod => write!(f, "%"),
            Self::Align => write!(f, "@"),
        }
    }
}

/// An expression.
///
/// This is generic so that different number types can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<T> {
    /// A base value.
    Value(T),
    /// A named constant.
    Const(Constant),
    /// A variable.
    Var(Variable),
    /// An expression `a b §`, where `§` is a [binary operator](BinOp).
    Op(Box<Expr<T>>, Box<Expr<T>>, BinOp),
    /// A dereferenced subexpression.
    Deref(Box<Expr<T>>),
}

impl<T: fmt::Display> fmt::Display for Expr<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Value(n) => write!(f, "{}", n),
            Self::Const(c) => write!(f, "{}", c),
            Self::Var(v) => write!(f, "{}", v),
            Self::Op(x, y, op) => write!(f, "{} {} {}", x, y, op),
            Self::Deref(x) => write!(f, "{} ^", x),
        }
    }
}

/// An assignment `v e =` where `v` is a [variable](Variable) and `e` is an [expression](Expr).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment<T>(Variable, Expr<T>);

impl<T: fmt::Display> fmt::Display for Assignment<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} =", self.0, self.1)
    }
}

pub mod parsing {
    //! Contains functions for parsing [expressions](super::Expr) and
    //! [assignments](super::Assignment).
    //!
    //! This is brought to you by `nom`.

    use super::*;
    use nom::branch::alt;
    use nom::bytes::complete::tag;
    use nom::character::complete::{alphanumeric1, digit1, space0};
    use nom::combinator::{all_consuming, map, map_res, not, opt, recognize, value};
    use nom::error::ParseError;
    use nom::multi::many0;
    use nom::sequence::{delimited, pair, preceded};
    use nom::{Err, Finish, IResult};
    use std::str::FromStr;

    /// The error kind for [`ExprParsingError`].
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ExprParsingErrorKind {
        /// An operator was encountered, but there were not enough operands on the stack.
        NotEnoughOperands,

        /// A variable was expected, but the identifier did not start with a `$`.
        IllegalVariableName,

        /// More than one expression preceded a `=`.
        MalformedAssignment,

        /// An error returned by `nom`.
        Nom(nom::error::ErrorKind),
    }

    /// An error encountered while parsing expressions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ExprParsingError<'a> {
        kind: ExprParsingErrorKind,
        input: &'a str,
    }

    impl<'a> ParseError<&'a str> for ExprParsingError<'a> {
        fn from_error_kind(input: &'a str, kind: nom::error::ErrorKind) -> Self {
            Self {
                input,
                kind: ExprParsingErrorKind::Nom(kind),
            }
        }

        fn append(_input: &'a str, _kind: nom::error::ErrorKind, other: Self) -> Self {
            other
        }
    }

    impl<'a, E> nom::error::FromExternalError<&'a str, E> for ExprParsingError<'a> {
        fn from_external_error(input: &'a str, kind: nom::error::ErrorKind, _e: E) -> Self {
            Self::from_error_kind(input, kind)
        }
    }

    /// Parses a [variable](super::Variable).
    fn variable(input: &str) -> IResult<&str, Variable, ExprParsingError> {
        let (input, _) = tag("$")(input).map_err(|_: nom::Err<ExprParsingError>| {
            nom::Err::Error(ExprParsingError {
                input,
                kind: ExprParsingErrorKind::IllegalVariableName,
            })
        })?;
        let (rest, var) = alphanumeric1(input)?;
        Ok((rest, Variable(format!("${}", var))))
    }

    /// Parses a [constant](super::Constant).
    fn constant(input: &str) -> IResult<&str, Constant, ExprParsingError> {
        let (input, _) = not(tag("$"))(input)?;
        let (rest, var) = alphanumeric1(input)?;
        Ok((rest, Constant(var.to_string())))
    }

    /// Parses a [binary operator](super::BinOp).
    fn bin_op(input: &str) -> IResult<&str, BinOp, ExprParsingError> {
        alt((
            value(BinOp::Add, tag("+")),
            value(BinOp::Sub, tag("-")),
            value(BinOp::Mul, tag("*")),
            value(BinOp::Div, tag("/")),
            value(BinOp::Mod, tag("%")),
            value(BinOp::Align, tag("@")),
        ))(input)
    }

    /// Parses an integer.
    fn number<T: FromStr>(input: &str) -> IResult<&str, T, ExprParsingError> {
        map_res(recognize(pair(opt(tag("-")), digit1)), |s: &str| {
            s.parse::<T>()
        })(input)
    }

    /// Parses a number, variable, or constant.
    fn base_expr<T: FromStr>(input: &str) -> IResult<&str, Expr<T>, ExprParsingError> {
        alt((
            map(number, Expr::Value),
            map(variable, Expr::Var),
            map(constant, Expr::Const),
        ))(input)
    }

    /// Parses a stack of expressions.
    ///
    /// # Example
    /// ```rust
    /// use symbolic_unwind::evaluator::Expr::*;
    /// use symbolic_unwind::evaluator::BinOp::*;
    /// # use symbolic_unwind::evaluator::parsing::expr;
    ///
    /// let (_, stack) = expr("1 2 + 3").unwrap();
    /// assert_eq!(stack.len(), 2);
    /// assert_eq!(stack[0], Op(Box::new(Value(1)), Box::new(Value(2)), Add));
    /// assert_eq!(stack[1], Value(3));
    /// ```
    pub fn expr<T: FromStr>(mut input: &str) -> IResult<&str, Vec<Expr<T>>, ExprParsingError> {
        let mut stack = Vec::new();

        while !input.is_empty() {
            if let Ok((rest, e)) = delimited(space0, base_expr, space0)(input) {
                stack.push(e);
                input = rest;
            } else if let Ok((rest, op)) = delimited(space0, bin_op, space0)(input) {
                let e2 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                let e1 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
                        }))
                    }
                };
                stack.push(Expr::Op(Box::new(e1), Box::new(e2), op));
                input = rest;
            } else if let Ok((rest, _)) =
                delimited::<_, _, _, _, ExprParsingError, _, _, _>(space0, tag("^"), space0)(input)
            {
                let e = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                stack.push(Expr::Deref(Box::new(e)));
                input = rest;
            } else {
                break;
            }
        }

        Ok((input, stack))
    }

    /// Parses an [assignment](Assignment).
    pub fn assignment<T: FromStr>(input: &str) -> IResult<&str, Assignment<T>, ExprParsingError> {
        let (input, v) = delimited(space0, variable, space0)(input)?;
        let (input, mut stack) = expr(input)?;

        // At this point there should be exactly one expression on the stack, otherwise
        // it's not a well-formed assignment.
        if stack.len() > 1 {
            return Err(Err::Error(ExprParsingError {
                input,
                kind: ExprParsingErrorKind::MalformedAssignment,
            }));
        }

        let e = match stack.pop() {
            Some(e) => e,
            None => {
                return Err(Err::Error(ExprParsingError {
                    input,
                    kind: ExprParsingErrorKind::NotEnoughOperands,
                }))
            }
        };

        let (rest, _) = preceded(space0, tag("="))(input)?;
        Ok((rest, Assignment(v, e)))
    }

    /// Parses a list of assignments.
    ///
    /// Will fail if there is any input remaining afterwards.
    pub fn assignments<T: FromStr + std::fmt::Debug>(
        input: &str,
    ) -> Result<Vec<Assignment<T>>, ExprParsingError> {
        let (_, assigns) =
            all_consuming(many0(delimited(space0, assignment, space0)))(input).finish()?;
        Ok(assigns)
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use nom::Finish;

        #[test]
        fn test_expr_1() {
            use Expr::*;
            let input = "1 2 + -3 *";
            let e = Op(
                Box::new(Op(Box::new(Value(1)), Box::new(Value(2)), BinOp::Add)),
                Box::new(Value(-3)),
                BinOp::Mul,
            );
            let (rest, parsed) = expr(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(parsed, vec![e]);
        }

        #[test]
        fn test_var() {
            let input = "$foo bar";
            let v = Variable(String::from("$foo"));
            let (rest, parsed) = variable(input).unwrap();
            assert_eq!(rest, " bar");
            assert_eq!(parsed, v);
        }

        #[test]
        fn test_expr_2() {
            use Expr::*;
            let input = "1 2 ^ + -3 $foo *";
            let e1 = Op(
                Box::new(Value(1)),
                Box::new(Deref(Box::new(Value(2)))),
                BinOp::Add,
            );
            let e2 = Op(
                Box::new(Value(-3)),
                Box::new(Var(Variable(String::from("$foo")))),
                BinOp::Mul,
            );
            let (rest, parsed) = expr(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(parsed, vec![e1, e2]);
        }

        #[test]
        fn test_expr_malformed() {
            let input = "3 +";
            let err = expr::<i8>(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExprParsingError {
                    input: "+",
                    kind: ExprParsingErrorKind::NotEnoughOperands,
                }
            );
        }

        #[test]
        fn test_assignment() {
            use Expr::*;
            let input = "$foo -4 ^ 7 @ =";
            let v = Variable("$foo".to_string());
            let e = Op(
                Box::new(Deref(Box::new(Value(-4)))),
                Box::new(Value(7)),
                BinOp::Align,
            );

            let (rest, a) = assignment(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(a, Assignment(v, e));
        }

        #[test]
        fn test_assignment_2() {
            use nom::multi::many1;
            use Expr::*;
            let input = "$foo -4 ^ = $bar baz 17 + = -42";
            let (v1, v2) = (Variable("$foo".to_string()), Variable("$bar".to_string()));
            let e1 = Deref(Box::new(Value(-4)));
            let e2 = Op(
                Box::new(Const(Constant("baz".to_string()))),
                Box::new(Value(17)),
                BinOp::Add,
            );

            let (rest, assigns) = many1(assignment)(input).unwrap();
            assert_eq!(rest, " -42");
            assert_eq!(assigns[0], Assignment(v1, e1));
            assert_eq!(assigns[1], Assignment(v2, e2));
        }

        #[test]
        fn test_assignment_malformed() {
            let input = "$foo -4 ^ 7 =";
            let err = assignment::<i8>(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExprParsingError {
                    input: "=",
                    kind: ExprParsingErrorKind::MalformedAssignment,
                }
            );
        }
    }
}
