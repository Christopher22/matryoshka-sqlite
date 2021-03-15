from typing import Optional

from system import System


class Matryoshka:
    """
    A wrapper around the shared library.
    """

    def __init__(self, path: str):
        self.library_path = path
        self.library = None

    def __enter__(self):
        try:
            self.library = System.identify().load(self.library_path)
        except OSError:
            self.library = None
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.library

    def __bool__(self):
        return self.library is not None

    @classmethod
    def find(cls, name: str = "matryoshka") -> Optional[str]:
        """
        Find a shared library by name.
        :param name: Name of the shared library
        :return: Path to the library or None if it is not found
        """

        system = System.identify()
        for path in system.dynamic_library_paths():
            file = path / f"{name}{system.dynamic_library_extension()}"
            if file.is_file():
                return str(file)
        return None
