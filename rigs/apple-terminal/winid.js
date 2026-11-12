// Prints the window number of Apple Terminal's largest on-screen window, so only that window is captured.
ObjC.import('CoreGraphics');
function run() {
  const arr = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0));
  let best = '', area = 0;
  for (let i = 0; i < arr.count; i++) {
    const w = arr.objectAtIndex(i);
    if (ObjC.unwrap(w.objectForKey('kCGWindowOwnerName')) !== 'Terminal') continue;
    const b = ObjC.deepUnwrap(w.objectForKey('kCGWindowBounds'));
    if (b.Width * b.Height > area) { area = b.Width * b.Height; best = ObjC.unwrap(w.objectForKey('kCGWindowNumber')) + ' ' + b.Width + 'x' + b.Height; }
  }
  return best;
}
