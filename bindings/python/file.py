import ctypes
from pathlib import Path
from typing import Sequence, Optional

from matryoshka import Matryoshka
from status import Status
from file_system import FileSystem
from exception import MatryoshkaException
from api_element import ApiElement


class File(ApiElement):
    """
    A single file stored in the virtual file system.
    """

    class FileHandle(ctypes.Structure):
        pass

    # The underlying type of handle
    HANDLE_TYPE = ctypes.POINTER(FileHandle)

    # The type of callback used for extracting found paths.
    FIND_CALLBACK = ctypes.CFUNCTYPE(None, ctypes.c_char_p)

    @classmethod
    def create(
        cls,
        file_system: FileSystem,
        virtual_path: Path,
        real_path: Path,
        chunk_size: int = -1,
    ) -> "File":
        """
        Create a new file in the virtual file system.
        :param file_system: The file system.
        :param virtual_path: The path in the virtual file system.
        :param real_path: The path of the real file on disk.
        :param chunk_size: The size of a chunk. Values < 0 will let the algorithm choose.
        :return: A opened file. Needs to be wrapped in a context manager!
        """

        with Status(file_system.matryoshka) as status:
            file_system.matryoshka.library.Push.restype = File.HANDLE_TYPE
            file_system.matryoshka.library.Push.argtypes = (
                FileSystem.HANDLE_TYPE,
                ctypes.c_char_p,
                ctypes.c_char_p,
                ctypes.c_int,
                ctypes.POINTER(Status.HANDLE_TYPE),
            )

            file_handle = file_system.matryoshka.library.Push(
                file_system.handle,
                "/".join(virtual_path.parts).encode("ascii"),
                str(real_path.absolute()).encode("ascii"),
                chunk_size,
                ctypes.byref(status.handle),
            )

            if not file_handle:
                exception = MatryoshkaException(status)
                raise exception

            return File(file_system, virtual_path, file_handle)

    @classmethod
    def find(cls, file_system: FileSystem, virtual_path: Path) -> Sequence["File"]:
        """
        Find all those file matching a glob pattern.
        :param file_system: The virtual file system.
        :param virtual_path: The path in the virtual file system which may contain glob-like expression.
        :return: A file which is not opened.
        """

        file_system.matryoshka.library.Find.restype = File.HANDLE_TYPE
        file_system.matryoshka.library.Find.argtypes = (
            FileSystem.HANDLE_TYPE,
            ctypes.c_char_p,
            File.FIND_CALLBACK,
        )

        paths = []

        def add_path(file_name: bytes) -> None:
            parts = file_name.decode(encoding="ascii").split("/")
            paths.append(Path(*parts))

        file_system.matryoshka.library.Find(
            file_system.handle,
            "/".join(virtual_path.parts).encode("ascii"),
            File.FIND_CALLBACK(add_path),
        )
        return [cls(file_system, path) for path in paths]

    def __init__(
        self,
        file_system: FileSystem,
        path: Path,
        existing_handle: Optional[HANDLE_TYPE] = None,
    ):
        """
        Create a new file. The file is not opened!
        :param file_system: The virtual file system.
        :param path: The path to the virtual file.
        :param existing_handle: An existing handle. If set, the instance will take ownership rather than open the file.
        """
        super().__init__(file_system.matryoshka)
        self.path = path
        self.file_system = file_system
        self.handle = (
            existing_handle if existing_handle is not None else File.HANDLE_TYPE()
        )

    @classmethod
    def initialize(cls, matryoshka: Matryoshka):
        matryoshka.library.Open.restype = File.HANDLE_TYPE
        matryoshka.library.Open.argtypes = (
            FileSystem.HANDLE_TYPE,
            ctypes.c_char_p,
            ctypes.POINTER(Status.HANDLE_TYPE),
        )

        matryoshka.library.DestroyFileHandle.argtypes = (File.HANDLE_TYPE,)

        matryoshka.library.Pull.restype = Status.HANDLE_TYPE
        matryoshka.library.Pull.argtypes = (
            FileSystem.HANDLE_TYPE,
            File.HANDLE_TYPE,
            ctypes.c_char_p,
        )

        matryoshka.library.GetSize.restype = ctypes.c_int
        matryoshka.library.GetSize.argtypes = (FileSystem.HANDLE_TYPE, File.HANDLE_TYPE)

    def __enter__(self):
        if not self.handle:
            with Status(self.file_system.matryoshka) as status:
                self.handle = self.matryoshka.library.Open(
                    self.file_system.handle,
                    "/".join(self.path.parts).encode("ascii"),
                    ctypes.byref(status.handle),
                )
                if not self.handle:
                    self.handle = File.HANDLE_TYPE()
                    raise MatryoshkaException(status)

        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.handle:
            self.matryoshka.library.DestroyFileHandle(self.handle)
            self.handle = File.HANDLE_TYPE()

    def __bool__(self) -> bool:
        return bool(self.handle)

    def __str__(self) -> str:
        return "/".join(self.path.parts)

    def pull(self, output_path: str) -> None:
        """
        Write a file into the real file system
        :param output_path: The output path on the real file system.
        """

        if not self:
            raise ValueError("The file is not open")

        with Status(
            self.matryoshka,
            self.matryoshka.library.Pull(
                self.file_system.handle, self.handle, str(output_path).encode("ascii")
            ),
        ) as status:
            if status:
                raise MatryoshkaException(status)

    @property
    def size(self) -> int:
        """
        Query the size of the file in virtual file system.
        :return: The size of the file in bytes or a value < 0 on error.
        """
        if not self:
            raise ValueError("The file is not open")

        return self.matryoshka.library.GetSize(self.file_system.handle, self.handle)
