# Changelog

## 1.1.0 (2026-11-18)

### Photos & Videos
- **New Photos & Videos tab.** A fast gallery of every picture and clip in the scan, grouped by month or by folder, with a year timeline, three tile sizes and a full-screen viewer with a filmstrip.
- **Finds copies that chat apps changed.** PrismDisk fingerprints what each photo shows, so versions that were resized, re-compressed, converted (PNG, HEIC, JPG), rotated or renamed are grouped with the original. Videos are matched by sampled frames and length.
- **Suggests what to keep.** Each group marks the best copy (highest resolution, sharpest, original metadata) and labels groups as exact copies, re-saved copies or merely similar shots. Nothing is marked for removal until you choose.
- **Smart filters** for screenshots, blurry photos, large videos and photos without a date taken.
- Date taken, camera and orientation are read from EXIF. Analysis runs in a separate low-priority helper and is cached, so the next visit is instant and the app stays light.

### Duplicates
- **Same content mode** (new default) finds byte-for-byte identical files whatever they are called, using a quick partial check before reading whole files. The old name and size mode is still one click away.
- New **100 KB** minimum size option.

### Safety
- **Encrypted container guard.** Deleting a VeraCrypt or TrueCrypt container, or a folder that holds one, now shows a clear warning and lists the containers found. Delete stays disabled until you confirm that everything inside will be lost.
- Containers are recognized by their `.hc`, `.tc` or `.vc` extension, or by a random-looking header with no known file signature, so renamed containers are caught too.

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
