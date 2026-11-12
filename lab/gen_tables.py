"""Write tables.rs: which block character shows which pattern of dots.

Bit i of the index is dot i, counting left to right, top to bottom.
Needs a Python whose unicodedata is Unicode 16 or newer (Python 3.14).
"""
import unicodedata as u


def look(name):
    try:
        return u.lookup(name)
    except KeyError:
        return None


QUAD = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█']

# Patterns that already had a character before the sextant and octant blocks were added.
SEXT_OLD = {'135': '▌', '246': '▐'}
OCT_OLD = {
    '1357': '▌', '2468': '▐', '1234': '▀', '5678': '▄',
    '13': '▘', '24': '▝', '57': '▖', '68': '▗', '1368': '▚', '2457': '▞',
    '123457': '▛', '123468': '▜', '135678': '▙', '245678': '▟',
    '78': '▂', '345678': '▆',
    '12': look('UPPER ONE QUARTER BLOCK'), '123456': look('UPPER THREE QUARTERS BLOCK'),
    '1': look('LEFT HALF UPPER ONE QUARTER BLOCK'), '2': look('RIGHT HALF UPPER ONE QUARTER BLOCK'),
    '7': look('LEFT HALF LOWER ONE QUARTER BLOCK'), '8': look('RIGHT HALF LOWER ONE QUARTER BLOCK'),
    '35': look('MIDDLE LEFT ONE QUARTER BLOCK'), '46': look('MIDDLE RIGHT ONE QUARTER BLOCK'),
}


def table(dots, prefix, old):
    out = []
    for p in range(1 << dots):
        pos = ''.join(str(i + 1) for i in range(dots) if p >> i & 1)
        c = ' ' if p == 0 else '█' if p == (1 << dots) - 1 else look(prefix + pos) or old.get(pos)
        assert c, f'no character for {prefix}{pos}'
        out.append(c)
    return out


def rust(name, t):
    return 'pub const %s: [char; %d] = [%s];\n' % (name, len(t), ', '.join("'\\u{%X}'" % ord(c) for c in t))


open('tables.rs', 'w').write(
    rust('QUAD', QUAD) + rust('SEXT', table(6, 'BLOCK SEXTANT-', SEXT_OLD)) + rust('OCT', table(8, 'BLOCK OCTANT-', OCT_OLD)))
