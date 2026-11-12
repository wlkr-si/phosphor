// Finds the test window: the largest on-screen window of one application (Apple Terminal
// unless told otherwise) that is not in a list of windows that were already open. Prints its
// x, y, width and height in screen pixels, then the screen's pixels per point. Prints nothing
// when there is no such window.
// usage: osascript -l JavaScript winrect.js [--owner Ghostty] [window numbers to skip...]
//        osascript -l JavaScript winrect.js [--owner Ghostty] --ids    (prints the numbers of its windows)
ObjC.import('AppKit');
ObjC.import('CoreGraphics');
function run(argv) {
  let owner = 'Terminal';
  const at = argv.indexOf('--owner');
  if (at >= 0) { owner = argv[at + 1]; argv = argv.slice(0, at).concat(argv.slice(at + 2)); }
  const arr = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0));
  const ids = [];
  let best = null, area = 0;
  for (let i = 0; i < arr.count; i++) {
    const w = arr.objectAtIndex(i);
    if (ObjC.unwrap(w.objectForKey('kCGWindowOwnerName')) !== owner) continue;
    if (ObjC.unwrap(w.objectForKey('kCGWindowLayer')) !== 0) continue;
    const id = String(ObjC.unwrap(w.objectForKey('kCGWindowNumber')));
    ids.push(id);
    if (argv.indexOf(id) >= 0) continue;
    const b = ObjC.deepUnwrap(w.objectForKey('kCGWindowBounds'));
    if (b.Width * b.Height > area) { area = b.Width * b.Height; best = b; }
  }
  if (argv[0] === '--ids') return ids.join(' ');
  if (!best) return '';
  const s = $.NSScreen.mainScreen;
  const k = s.backingScaleFactor;
  const sw = s.frame.size.width, sh = s.frame.size.height;
  // only the main screen is filmed; a window reaching past it cannot be cropped safely
  if (best.X < 0 || best.Y < 0 || best.X + best.Width > sw || best.Y + best.Height > sh) return '';
  const px = v => Math.round(v * k);
  return [px(best.X), px(best.Y), px(best.Width), px(best.Height), k].join(' ');
}
