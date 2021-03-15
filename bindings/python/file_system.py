import ctypes

from matryoshka import Matryoshka
from status import Status
from exception import MatryoshkaException
from api_element import ApiElement


class FileSystem(ApiElement):
    """
    The entry point for accessing the file system in a SQlite database.
    """

    class FileSystem(ctypes.Structure):
        pass

    # The underlying type of handle
    HANDLE_TYPE = ctypes.POINTER(FileSystem)

    def __init__(self, path: str, matryoshka: Matryoshka):
        """
        Create an instance referring to an existing SQLite database.
        WARNING: The file system is not yet open nor created! Use __enter__.

        :param path: The path to the database.
        :param matryoshka: The shared library.
        """

        super().__init__(matryoshka)
        self.path = path
        self.handle = FileSystem.HANDLE_TYPE()

    @classmethod
    def initialize(cls, matryoshka: Matryoshka):
        matryoshka.library.Load.restype = FileSystem.HANDLE_TYPE
        matryoshka.library.Load.argtypes = [
            ctypes.c_char_p,
            ctypes.POINTER(Status.HANDLE_TYPE),
        ]

        matryoshka.library.DestroyFileSystem.argtypes = [
            ctypes.POINTER(FileSystem.FileSystem)
        ]

    def __enter__(self):
        if not self.handle:
            with Status(self.matryoshka) as status:
                self.handle = self.matryoshka.library.Load(
                    self.path.encode("ascii"), ctypes.byref(status.handle)
                )
                if not self.handle:
                    self.handle = Status.HANDLE_TYPE()
                    raise MatryoshkaException(status)

        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if bool(self):
            self.matryoshka.library.DestroyFileSystem(self.handle)
            self.handle = FileSystem.HANDLE_TYPE()

    def __bool__(self):
        return bool(self.handle)
