import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "tools"))

from extract_externs import Function, UnsupportedAbi, mire_type, render


class MireTypeTests(unittest.TestCase):
    def test_integer_boolean_and_pointer_types(self):
        self.assertEqual(mire_type("int64_t"), "i64")
        self.assertEqual(mire_type("uint32_t"), "u32")
        self.assertEqual(mire_type("_Bool"), "bool")
        self.assertEqual(mire_type("const char *"), "&str")
        self.assertEqual(mire_type("char *"), "ptr")
        self.assertEqual(mire_type("const void *"), "ptr")

    def test_void_is_only_valid_as_a_return_type(self):
        self.assertEqual(mire_type("void", result=True), "")
        with self.assertRaises(UnsupportedAbi):
            mire_type("void")

    def test_aggregate_and_callback_abis_are_rejected(self):
        for c_type in ("struct pal_handle", "struct pal_bytes_t", "void *(*)(void *)"):
            with self.subTest(c_type=c_type), self.assertRaises(UnsupportedAbi):
                mire_type(c_type)

    def test_renderer_keeps_supported_signatures_and_reports_the_rest(self):
        functions = [
            Function("pal_read", "int64_t", (("buffer", "void *"), ("length", "int64_t"))),
            Function("pal_open", "struct pal_handle", (("path", "const char *"),)),
        ]
        source, unsupported = render(functions)
        self.assertIn('extern fn pal_read: (buffer :ptr, length :i64) :i64 lib "pal"', source)
        self.assertEqual(len(unsupported), 1)
        self.assertIn("struct pal_handle", unsupported[0])


if __name__ == "__main__":
    unittest.main()
