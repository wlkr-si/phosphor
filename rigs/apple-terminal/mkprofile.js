// Writes an Apple Terminal settings file: a window of a given font size and grid that runs one command and closes.
// usage: osascript -l JavaScript mkprofile.js out.terminal name font size cols rows command
ObjC.import('AppKit');
function run(argv) {
  const [out, name, fontName, size, cols, rows, cmd] = argv;
  const arch = o => $.NSKeyedArchiver.archivedDataWithRootObject(o);
  const d = $.NSMutableDictionary.alloc.init;
  d.setObjectForKey(name, 'name');
  d.setObjectForKey('Window Settings', 'type');
  d.setObjectForKey(arch($.NSFont.fontWithNameSize(fontName, parseFloat(size))), 'Font');
  d.setObjectForKey(arch($.NSColor.colorWithSRGBRedGreenBlueAlpha(0, 0, 0, 1)), 'BackgroundColor');
  d.setObjectForKey(arch($.NSColor.colorWithSRGBRedGreenBlueAlpha(0.8, 0.8, 0.8, 1)), 'TextColor');
  d.setObjectForKey($.NSNumber.numberWithInt(parseInt(cols)), 'columnCount');
  d.setObjectForKey($.NSNumber.numberWithInt(parseInt(rows)), 'rowCount');
  d.setObjectForKey($.NSNumber.numberWithDouble(1.0), 'FontWidthSpacing');
  d.setObjectForKey($.NSNumber.numberWithDouble(1.0), 'FontHeightSpacing');
  d.setObjectForKey($.NSNumber.numberWithBool(false), 'FontAntialias');
  d.setObjectForKey(cmd, 'CommandString');
  d.setObjectForKey($.NSNumber.numberWithBool(false), 'RunCommandAsShell');
  d.setObjectForKey($.NSNumber.numberWithInt(0), 'shellExitAction');
  d.setObjectForKey($.NSNumber.numberWithBool(false), 'ShowWindowSettingsNameInTitle');
  d.writeToFileAtomically(out, true);
  return 'ok';
}
