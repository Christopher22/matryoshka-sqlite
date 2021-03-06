import platform
from enum import Enum
import os
from pathlib import Path
from typing import Iterable, Union
import ctypes


class System(Enum):
    """
    Abstraction over different operation systems for the handling of shared libraries
    """

    Windows = "Windows"
    Linux = "Linux"
    MacOS = "Darwin"

    @staticmethod
    def identify() -> "System":
        plt = platform.system()
        for system in System:
            if system.value == plt:
                return system
        raise OSError(f"Unsupported operation system '{plt}'")

    def load(self, path: Union[Path, str]):
        return ctypes.WinDLL(path) if self == System.Windows else ctypes.CDLL(path)

    def dynamic_library_name(self, name: str) -> str:
        return f"{self.dynamic_library_prefix()}{name}{self.dynamic_library_suffix()}"

    def dynamic_library_prefix(self) -> str:
        return "" if self == System.Windows else "lib"

    def dynamic_library_suffix(self) -> str:
        if self == System.Windows:
            return ".dll"
        elif self == System.Linux:
            return ".so"
        elif self == System.MacOS:
            return ".dylib"
        else:
            raise NotImplementedError("Unsupported OS")

    def dynamic_library_env(self) -> str:
        if self == System.Windows:
            return "PATH"
        elif self == System.Linux:
            return "LD_LIBRARY_PATH"
        elif self == System.MacOS:
            return "DYLD_LIBRARY_PATH"
        else:
            raise NotImplementedError("Unsupported OS")

    def dynamic_library_paths(self) -> Iterable[Path]:
        for path in os.environ[self.dynamic_library_env()].split(os.pathsep):
            path = Path(path)
            if path.is_dir():
                yield path.absolute()
