import unittest
from pathlib import Path
import faulthandler
from tempfile import TemporaryDirectory

from matryoshka import Matryoshka
from file_system import FileSystem
from file import File


class TestMatryoshka(unittest.TestCase):
    def setUp(self) -> None:
        dll_path = Matryoshka.find()
        if dll_path is None:
            raise ValueError(
                "Please add matryoshka to your directory for shared libraries"
            )

        self.matryoshka = Matryoshka(str(dll_path)).__enter__()

        # Create the example file and fill it with content
        self.temp_directory = TemporaryDirectory()
        self.example_file = Path(self.temp_directory.name, r"example_file")
        with self.example_file.open("w+b") as tmp_file:
            tmp_file.write(b"1234")

    def tearDown(self) -> None:
        self.matryoshka.__exit__(None, None, None)

        self.example_file = None
        self.temp_directory.cleanup()

    def test_create(self):
        example_path = Path("folder1", "file")
        with FileSystem(":memory:", self.matryoshka) as fs:
            with File.create(fs, example_path, self.example_file) as file:
                self.assertEqual(file.size, 4)
                self.assertEqual(str(file), "/".join(example_path.parts))

    def test_load(self):
        output_file = Path(self.temp_directory.name, "loaded_example_file")
        example_path = Path("folder1", "file")

        with FileSystem(":memory:", self.matryoshka) as fs:
            with File.create(fs, example_path, self.example_file):
                pass
            with File(fs, example_path) as file:
                file.pull(output_file)

        with output_file.open("rb") as output:
            self.assertEqual(output.read(), b"1234")

        output_file.unlink()

    def test_find(self):
        with FileSystem(":memory:", self.matryoshka) as fs:
            with File.create(fs, Path("folder1", "file"), self.example_file):
                pass
            with File.create(fs, Path("folder2", "file"), self.example_file):
                pass

            files = File.find(fs, Path("folder*", "file"))
            self.assertEqual(len(files), 2)


if __name__ == "__main__":
    faulthandler.enable()
    unittest.main()
