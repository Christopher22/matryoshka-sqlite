from matryoshka import Matryoshka


class ApiElement:
    """
    An API element which may modify the shared library.
    """

    def __init__(self, matryoshka: Matryoshka):
        if not matryoshka:
            raise ValueError("The shared library is not loaded!")

        self.matryoshka = matryoshka
        self.initialize(self.matryoshka)

    @classmethod
    def initialize(cls, matryoshka: Matryoshka):
        """
        Overwriting this method may be used to modify the shared Matryoshka library.
        :param matryoshka: The shared library.
        """
        pass
