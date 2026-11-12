#!/usr/bin/env python3
"""Extract PAL C declarations that have an exact Mire FFI representation.

The tool deliberately rejects ABI shapes it cannot represent. In particular,
it never guesses that a by-value struct or callback is an integer or pointer.
Clang is used as the C parser so comments, multiline declarations, typedefs,
and nested function-pointer syntax are handled by the language frontend.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class Function:
    name: str
    result: str
    parameters: tuple[tuple[str, str], ...]


class UnsupportedAbi(ValueError):
    pass


SCALARS = {
    "void": "",
    "_Bool": "bool",
    "bool": "bool",
    "char": "i8",
    "signed char": "i8",
    "unsigned char": "u8",
    "short": "i16",
    "short int": "i16",
    "unsigned short": "u16",
    "unsigned short int": "u16",
    "int": "i32",
    "long": "i64",
    "long int": "i64",
    "unsigned long": "u64",
    "unsigned long int": "u64",
    "unsigned": "u32",
    "unsigned int": "u32",
    "int8_t": "i8",
    "uint8_t": "u8",
    "int16_t": "i16",
    "uint16_t": "u16",
    "int32_t": "i32",
    "uint32_t": "u32",
    "int64_t": "i64",
    "uint64_t": "u64",
    "int64_t *": "ptr",
    "uint64_t *": "ptr",
    "int32_t *": "ptr",
    "uint32_t *": "ptr",
}


def clang_ast(header: Path, clang: str) -> dict[str, Any]:
    command = [
        clang,
        "-x",
        "c",
        "-std=c11",
        "-Isrc/pal",
        "-Isrc/runtime",
        "-Xclang",
        "-ast-dump=json",
        "-fsyntax-only",
        str(header),
    ]
    result = subprocess.run(command, text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "clang failed to parse PAL header")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"clang produced invalid AST JSON: {error}") from error


def declarations(ast: dict[str, Any]) -> list[Function]:
    aliases = {
        node["name"]: node.get("type", {}).get("desugaredQualType", node.get("type", {}).get("qualType", ""))
        for node in ast.get("inner", [])
        if node.get("kind") == "TypedefDecl" and node.get("name", "").startswith("pal_")
    }
    found: dict[str, Function] = {}
    for node in ast.get("inner", []):
        if node.get("kind") != "FunctionDecl":
            continue
        name = node.get("name", "")
        if not name.startswith("pal_"):
            continue
        signature = node.get("type", {}).get("qualType", "")
        # FunctionDecl children include ParmVarDecl nodes in declaration order.
        parameters = []
        for index, child in enumerate(node.get("inner", [])):
            if child.get("kind") != "ParmVarDecl":
                continue
            type_info = child.get("type", {})
            parameter_type = type_info.get("qualType", "")
            if type_info.get("typeAliasDeclId"):
                parameter_type = type_info.get("desugaredQualType", parameter_type)
            parameters.append((child.get("name") or f"arg{index}", parameter_type))
        result_type = signature.split("(", 1)[0].strip()
        result_type = aliases.get(result_type, result_type)
        found[name] = Function(name, result_type, tuple(parameters))
    return sorted(found.values(), key=lambda function: function.name)


def mire_type(c_type: str, *, result: bool = False) -> str:
    c_type = " ".join(c_type.split())
    if c_type == "const char *":
        return "ptr" if result else "&str"
    normalized = " ".join(c_type.replace("const ", "").split())
    if normalized in SCALARS:
        mapped = SCALARS[normalized]
        if result and not mapped:
            return ""
        if not result and not mapped:
            raise UnsupportedAbi("void parameter")
        return mapped
    if normalized in {"char *", "signed char *", "unsigned char *"}:
        return "ptr"
    if normalized.endswith("*"):
        # All non-string pointers are opaque addresses in the current Mire FFI.
        # Function pointers need a distinct ABI representation and are rejected.
        if "(" in normalized or ")" in normalized:
            raise UnsupportedAbi("callback/function pointer")
        return "ptr"
    raise UnsupportedAbi(f"unsupported C ABI type `{c_type}`")


def render(functions: list[Function]) -> tuple[str, list[str]]:
    lines = [
        "// Generated from src/pal/pal.h by palace/tools/extract_externs.py.",
        "// Do not edit by hand; unsupported ABI declarations are listed separately.",
        "module pal_externs",
        "",
    ]
    unsupported: list[str] = []
    for function in functions:
        try:
            result_type = mire_type(function.result, result=True)
            parameters = ", ".join(
                f"{name} :{mire_type(parameter_type)}"
                for name, parameter_type in function.parameters
            )
        except UnsupportedAbi as error:
            args = ", ".join(parameter_type for _, parameter_type in function.parameters)
            unsupported.append(
                f"{function.name}: {function.result} ({args}) -- {error}"
            )
            continue
        result = f" :{result_type}" if result_type else ""
        lines.append(f'extern fn {function.name}: ({parameters}){result} lib "pal"')
    lines.append("")
    return "\n".join(lines), unsupported


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("header", nargs="?", type=Path, default=Path("src/pal/pal.h"))
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--output", type=Path, help="write generated Mire declarations here")
    parser.add_argument("--unsupported", type=Path, help="write unsupported ABI inventory")
    parser.add_argument("--check", action="store_true", help="fail if any PAL declaration is unsupported")
    parser.add_argument("--quiet", action="store_true", help="print only the supported/total summary")
    args = parser.parse_args()

    try:
        functions = declarations(clang_ast(args.header, args.clang))
        output, unsupported = render(functions)
    except (OSError, RuntimeError) as error:
        print(f"extract_externs: {error}", file=sys.stderr)
        return 2

    if args.output:
        args.output.write_text(output)
    else:
        sys.stdout.write(output)
    if args.unsupported:
        args.unsupported.write_text("\n".join(unsupported) + ("\n" if unsupported else ""))
    print(
        f"extract_externs: {len(functions) - len(unsupported)}/{len(functions)} "
        "PAL declarations represented exactly",
        file=sys.stderr,
    )
    if not args.quiet:
        for declaration in unsupported:
            print(f"unsupported: {declaration}", file=sys.stderr)
    if args.check and unsupported:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
