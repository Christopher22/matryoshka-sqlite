import ctypes
from typing import Optional

from matryoshka import Matryoshka
from api_element import ApiElement


class Status(ApiElement):
    """
    A status reported by the shared library. Mostly for internal use.
    """

    class Status(ctypes.Structure):
        pass

    # The underlying type of handle
    HANDLE_TYPE = ctypes.POINTER(Status)

    def __init__(self, matryoshka: Matryoshka, handle: Optional[HANDLE_TYPE] = None):
        """
        Create a new status.
        :param matryoshka: The shared library.
        :param handle: A existing handle. If valid, the instance takes ownership and release it.
        """

        super().__init__(matryoshka)
        self.handle = handle if handle is not None else Status.HANDLE_TYPE()

    @classmethod
    def initialize(cls, matryoshka: Matryoshka):
        matryoshka.library.GetMessage.argtypes = [Status.HANDLE_TYPE]
        matryoshka.library.GetMessage.restype = ctypes.c_char_p

        matryoshka.library.DestroyStatus.argtypes = [Status.HANDLE_TYPE]

    def __str__(self) -> str:
        if not self.handle:
            return "<Uninitialized>"

        raw_str: bytes = self.matryoshka.library.GetMessage(self.handle)
        return raw_str.decode(encoding="ascii")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.handle:
            self.matryoshka.library.DestroyStatus(self.handle)
            self.handle = Status.HANDLE_TYPE()

    def __bool__(self):
        return bool(self.handle)
