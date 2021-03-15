from status import Status


class MatryoshkaException(Exception):
    """
    A exception thrown on failing operations.
    """

    def __init__(self, status: Status):
        super().__init__(str(status))
