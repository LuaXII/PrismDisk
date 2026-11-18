# Changelog

## 1.0.0 (2026-11-18)

The first public release of PrismDisk.

### Scanning
- **Instant NTFS scans** that read the Master File Table directly, mapping millions of files in seconds.
- **Folder walk fallback** for non-NTFS drives such as USB sticks, and for runs without administrator rights.
- **Scan a single folder**, a whole drive, or open a WizTree / PrismDisk **CSV export**.

### Exploring
- **Tree view** with allocated size, logical size, % of parent, file and folder counts and modified dates.
- **Treemap** with cushion shading, hover inspector and double-click zoom, plus a **sunburst** view.
- **File types** panel: click a type to highlight it everywhere.
- **Largest files & search** with wildcards, path patterns, `ext:` filters, minimum size and age filters.
- **Duplicate finder** ranked by wasted space.
- **Smart cleanup** for temp folders, shader, browser and package-manager caches, crash dumps and the Recycle Bin.

### Everything else
- Move to Recycle Bin or delete permanently, with protection for critical system folders.
- “Analyze with PrismDisk” in the Explorer right-click menu.
- Export to CSV in WizTree's format.
- Native app: one process, about 100 MB of memory, no browser engine.
- Custom setup wizard and uninstaller, plus silent install for scripting.
- Automatic updates with release notes.
