import typing

__all__: typing.Sequence[str] = ("Exp", "JSONValue", "VarAccess")

JSONValue = int | float | str | bool | None | typing.Sequence["JSONValue"] | typing.Mapping[str, "JSONValue"]
"""Represents a valid JSON value."""

class Exp:
    """Represents an expression that can be evaluated with variable bindings.

    The expression can be evaluated with a set of variable bindings to produce a JSON value.
    """

    def __init__(self, exp: str, /) -> None:
        """Create a new expression from a string. The string should be a valid expression.

        Parameters
        ----------
        exp : str
            The expression string. It should be a valid expression.
        """

    def eval(self, bindings: typing.Mapping[str, JSONValue]) -> JSONValue:
        """Evaluate the expression with the given bindings.

        Parameters
        ----------
        bindings : Mapping[str, JSONValue]
            The bindings for the variables in the expression.

        Returns
        -------
        JSONValue
            The result of the evaluation.
        """

class VarAccess:
    """Represents a variable access that can be used to access values from JSON objects or bindings."""

    def __init__(self, accessor: str, /) -> None:
        """Create a new variable access.

        Parameters
        ----------
        accessor : str
            The accessor string. It should be a valid accessor string, which is a dot-separated
            string of variable names and indices (e.g., "a.b[0].c").
        """

    def access(self, value: JSONValue, /) -> JSONValue:
        """Use the accessor to access a value from the given JSON value.

        Parameters
        ----------
        value : JSONValue
            The JSON value to access.

        Returns
        -------
        JSONValue
            The accessed value.
        """

    def replace(self, value: JSONValue, replacement: JSONValue, /) -> JSONValue:
        """Use the accessor to replace a value in the given JSON value with the replacement value.

        Parameters
        ----------
        value : JSONValue
            The JSON value to access and replace.
        replacement : JSONValue
            The value to replace the accessed value with.

        Returns
        -------
        JSONValue
            The old value that was replaced.
        """

    def access_bindings(self, bindings: typing.Mapping[str, JSONValue], /) -> JSONValue:
        """Use the accessor to access a value from the given bindings.

        Parameters
        ----------
        bindings : Mapping[str, JSONValue]
            The bindings to access.

        Returns
        -------
        JSONValue
            The accessed value.
        """
