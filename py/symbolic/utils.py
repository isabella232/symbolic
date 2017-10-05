import uuid
import ntpath
import posixpath
from symbolic._lowlevel import ffi, lib
from symbolic._compat import text_type, NUL
from symbolic.exceptions import exceptions_by_code, SymbolicError


__all__ = ['common_path_join', 'strip_common_path_prefix']


def _is_win_path(x):
    return '\\' in x or (ntpath.isabs(x) and not posixpath.isabs(x))


def common_path_join(a, b):
    """Joins two paths together while guessing the platform (win vs unix)."""
    if _is_win_path(a):
        return ntpath.normpath(ntpath.join(a, b))
    return posixpath.join(a, b)


def strip_common_path_prefix(base, prefix):
    """Strips `prefix` from `a`."""
    if _is_win_path(base):
        path = ntpath
    else:
        path = posixpath
    pieces_a = path.normpath(base).split(path.sep)
    pieces_b = path.normpath(prefix).split(path.sep)
    if pieces_a[:len(pieces_b)] == pieces_b:
        return path.sep.join(pieces_a[len(pieces_b):])
    return path.normpath(base)


class RustObject(object):
    __dealloc_func__ = None
    _objptr = None

    def __init__(self):
        raise TypeError('Cannot instanciate %r objects' %
                        self.__class__.__name__)

    @classmethod
    def _from_objptr(cls, ptr):
        rv = object.__new__(cls)
        rv._objptr = ptr
        return rv

    def _methodcall(self, func, *args):
        return rustcall(func, self._get_objptr(), *args)

    def _get_objptr(self):
        if not self._objptr:
            raise RuntimeError('Object is closed')
        return self._objptr

    def __del__(self):
        if self._objptr is None:
            return
        f = self.__class__.__dealloc_func__
        if f is not None:
            rustcall(f, self._objptr)
            self._objptr = None


def rustcall(func, *args):
    """Calls rust method and does some error handling."""
    lib.symbolic_err_clear()
    rv = func(*args)
    err = lib.symbolic_err_get_last_code()
    if not err:
        return rv
    msg = lib.symbolic_err_get_last_message()
    cls = exceptions_by_code.get(err, SymbolicError)
    raise cls(decode_str(msg))


def decode_str(s):
    """Decodes a SymbolicStr"""
    return ffi.unpack(s.data, s.len).decode('utf-8')


def encode_str(s):
    """Encodes a SymbolicStr"""
    rv = ffi.new('SymbolicStr *')
    if isinstance(s, text_type):
        s = s.encode('utf-8')
    rv[0].data = ffi.from_buffer(s)
    rv[0].len = len(s)
    return rv


def encode_path(s):
    """Encodes a path value."""
    if isinstance(s, text_type):
        s = s.encode('utf-8')
    if NUL in s:
        raise TypeError('Null bytes are not allowed in paths')
    return s


def decode_uuid(value):
    """Decodes the given uuid value."""
    return uuid.UUID(bytes=ffi.string(value.data))


def make_uuid(value):
    """Converts a value into a python uuid object."""
    if isinstance(value, uuid.UUID):
        return value
    return uuid.UUID(value)