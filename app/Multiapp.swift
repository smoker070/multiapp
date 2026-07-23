// Multiapp — macOS menu-bar app (v0.1.0)
// Thin GUI over the `multiapp` CLI engine (the CLI stays the source of truth).
// Build: see build.sh in this folder. No Xcode required — plain swiftc + AppKit.
import AppKit

// ---------------------------------------------------------------- CLI bridge
struct CLI {
    /// Resolve the engine: prefer the move-proof stub target, then the stub itself.
    static let path: String = {
        let conf = NSHomeDirectory() + "/.config/multiapp/target"
        if let t = try? String(contentsOfFile: conf, encoding: .utf8),
           FileManager.default.isExecutableFile(atPath: t.trimmingCharacters(in: .whitespacesAndNewlines)) {
            return t.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return NSHomeDirectory() + "/.local/bin/multiapp"
    }()

    @discardableResult
    static func run(_ args: [String], input: String? = nil) -> (out: String, err: String, code: Int32) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/bash")
        p.arguments = [path] + args
        let outPipe = Pipe(), errPipe = Pipe()
        p.standardOutput = outPipe; p.standardError = errPipe
        if let input = input {
            let inPipe = Pipe()
            p.standardInput = inPipe
            inPipe.fileHandleForWriting.write(input.data(using: .utf8)!)
            inPipe.fileHandleForWriting.closeFile()
        }
        do { try p.run() } catch { return ("", "cannot run \(path): \(error)", 127) }
        let out = String(data: outPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        p.waitUntilExit()
        return (out, err, p.terminationStatus)
    }

    struct Profile { let app: String; let name: String; let running: Bool }
    struct App { let key: String; let display: String; let verdict: String }

    static func profiles() -> [Profile] {
        run(["list", "--raw"]).out.split(separator: "\n").compactMap {
            let f = $0.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
            guard f.count >= 3 else { return nil }
            return Profile(app: f[0], name: f[1], running: f[2] == "running")
        }
    }

    struct Session { let index: Int; let date: String; let cwd: String; let title: String }

    static func sessions(of profile: String) -> [Session] {
        run(["sessions", "claude", profile, "--raw"]).out
            .split(separator: "\n").enumerated().compactMap { (i, line) in
                let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
                guard f.count >= 5 else { return nil }
                return Session(index: i + 1, date: f[2], cwd: f[3], title: f[4])
            }
    }

    static func apps() -> [App] {
        // parse `multiapp apps` fixed-width table; keep launchable verdicts only
        run(["apps"]).out.split(separator: "\n").dropFirst().compactMap { line in
            let s = String(line)
            guard s.count > 45 else { return nil }
            let key = String(s.prefix(20)).trimmingCharacters(in: .whitespaces)
            let display = String(s.dropFirst(21).prefix(24)).trimmingCharacters(in: .whitespaces)
            let verdict = String(s.dropFirst(46).prefix(11)).trimmingCharacters(in: .whitespaces)
            // GUI picker: full-value apps only — "partial" (e.g. ChatGPT: account shared
            // across profiles) is excluded from the picker by design; the CLI still allows it
            guard ["supported", "untested"].contains(verdict) else { return nil }
            return App(key: key, display: display, verdict: verdict)
        }
    }

    struct BackupApp { let key: String; let verdict: String; let authLocus: String }

    /// Apps whose real data can be backed up on this machine (from `migrate-list --raw`).
    static func backupApps() -> [BackupApp] {
        run(["migrate-list", "--raw"]).out.split(separator: "\n").compactMap {
            let f = $0.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
            guard f.count >= 3 else { return nil }
            return BackupApp(key: f[0], verdict: f[1], authLocus: f[2])
        }
    }

    static func displayName(_ key: String) -> String {
        // parse the FULL apps table (all verdicts) — apps() filters out "partial" apps like ChatGPT
        for line in run(["apps"]).out.split(separator: "\n").dropFirst() {
            let s = String(line)
            guard s.count > 21 else { continue }
            let k = String(s.prefix(20)).trimmingCharacters(in: .whitespaces)
            if k == key { return String(s.dropFirst(21).prefix(24)).trimmingCharacters(in: .whitespaces) }
        }
        return key
    }

    static func isRunning(app key: String) -> Bool {
        let disp = displayName(key)
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        p.arguments = ["-e", "application \"\(disp)\" is running"]
        let pipe = Pipe(); p.standardOutput = pipe; p.standardError = Pipe()
        do { try p.run() } catch { return false }
        let out = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        p.waitUntilExit()
        return out.trimmingCharacters(in: .whitespacesAndNewlines) == "true"
    }

    static func quit(app key: String) {
        let disp = displayName(key)
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        p.arguments = ["-e", "tell application \"\(disp)\" to quit"]
        try? p.run(); p.waitUntilExit()
    }
}

// ---------------------------------------------------------------- shared UI helpers
final class FlippedView: NSView { override var isFlipped: Bool { true } }
final class Handler: NSObject { let f: () -> Void; init(_ f: @escaping () -> Void) { self.f = f }; @objc func fire() { f() } }

/// The app's one dialog style: centered logo, centered bold title, centered gray
/// message, stacked full-width controls/buttons. (macOS 26 NSAlert is left-aligned
/// with no alignment API — hence our own.)
final class CenteredDialog {
    let W: CGFloat, pad: CGFloat = 30, cw: CGFloat
    let win: NSWindow
    let root: FlippedView
    private var y: CGFloat = 34
    private var handlers: [Handler] = []
    private(set) var clicked = 0

    init(title: String, message: String? = nil, width: CGFloat = 340) {
        W = width; cw = W - 2 * pad
        win = NSWindow(contentRect: NSRect(x: 0, y: 0, width: W, height: 100),
                       styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        win.titleVisibility = .hidden
        win.titlebarAppearsTransparent = true
        win.isMovableByWindowBackground = true
        root = FlippedView(frame: NSRect(x: 0, y: 0, width: W, height: 100))
        win.contentView = root

        let iconView = NSImageView(frame: NSRect(x: (W-64)/2, y: y, width: 64, height: 64))
        iconView.image = NSApp.applicationIconImage
        root.addSubview(iconView); y += 76

        let t = NSTextField(labelWithString: title)
        t.font = .boldSystemFont(ofSize: 15)
        t.alignment = .center
        t.frame = NSRect(x: pad, y: y, width: cw, height: 20)
        root.addSubview(t); y += 28

        if let message, !message.isEmpty {
            let font = NSFont.systemFont(ofSize: 12)
            let h = max(18, ceil((message as NSString).boundingRect(
                with: CGSize(width: cw, height: 2000),
                options: [.usesLineFragmentOrigin], attributes: [.font: font]).height) + 4)
            let body = NSTextField(wrappingLabelWithString: message)
            body.font = font
            body.textColor = .secondaryLabelColor
            body.alignment = .center
            body.frame = NSRect(x: pad, y: y, width: cw, height: h)
            root.addSubview(body); y += h + 12
        }
    }

    func add(_ v: NSView, height: CGFloat, gap: CGFloat = 8) {
        v.frame = NSRect(x: pad, y: y, width: cw, height: height)
        root.addSubview(v); y += height + gap
    }

    func addButton(_ title: String, code: Int, key: String = "") {
        let h = Handler { [weak self] in self?.clicked = code; NSApp.stopModal() }
        handlers.append(h)
        let b = NSButton(title: title, target: h, action: #selector(Handler.fire))
        b.bezelStyle = .rounded
        b.keyEquivalent = key
        b.frame = NSRect(x: pad, y: y, width: cw, height: 30)
        root.addSubview(b); y += 34
    }

    func run(firstResponder: NSView? = nil) -> Int {
        win.setContentSize(NSSize(width: W, height: y + 8))
        win.center()
        if let fr = firstResponder { win.initialFirstResponder = fr }
        NSApp.activate(ignoringOtherApps: true)
        win.makeKeyAndOrderFront(nil)
        NSApp.runModal(for: win)
        win.orderOut(nil)
        return clicked
    }
}

// ---------------------------------------------------------------- app delegate
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    var statusItem: NSStatusItem!
    let menu = NSMenu()

    func applicationDidFinishLaunching(_ n: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let img = NSImage(systemSymbolName: "square.3.layers.3d", accessibilityDescription: "Multiapp") {
            img.isTemplate = true
            statusItem.button?.image = img
        } else {
            statusItem.button?.title = "M"
        }
        menu.delegate = self
        statusItem.menu = menu
        rebuild()
    }

    func menuWillOpen(_ menu: NSMenu) { rebuild() }

    func rebuild() {
        menu.removeAllItems()
        let profiles = CLI.profiles()
        if profiles.isEmpty {
            menu.addItem(withTitle: "No profiles yet", action: nil, keyEquivalent: "")
        }
        var lastApp = ""
        for p in profiles {
            if p.app != lastApp {
                if !lastApp.isEmpty { menu.addItem(.separator()) }
                let header = NSMenuItem(title: p.app.uppercased(), action: nil, keyEquivalent: "")
                header.isEnabled = false
                menu.addItem(header)
                lastApp = p.app
            }
            let item = NSMenuItem(title: "\(p.running ? "●" : "○") \(p.name)\(p.running ? "  (running)" : "")",
                                  action: nil, keyEquivalent: "")
            let sub = NSMenu()
            sub.autoenablesItems = false   // without this AppKit force-enables every actioned item
            let launch = make("Launch", #selector(launch(_:)), rep: p)
            launch.isEnabled = !p.running
            sub.addItem(launch)
            let stop = make("Stop", #selector(stop(_:)), rep: p)
            stop.isEnabled = p.running
            sub.addItem(stop)
            sub.addItem(.separator())
            for (title, sel) in [("Rename Profile…", #selector(renameProfile(_:))),
                                 ("Clone Profile…", #selector(cloneProfile(_:))),
                                 ("Export Profile…", #selector(exportProfile(_:)))] {
                let i = make(title, sel, rep: p)
                i.isEnabled = !p.running       // CLI refuses these on running profiles anyway
                sub.addItem(i)
            }
            if p.app == "claude" {
                sub.addItem(.separator())
                sub.addItem(make("Transfer Sessions…", #selector(transferSessions(_:)), rep: p))
            }
            sub.addItem(.separator())
            let del = make("Delete Profile…", #selector(deleteProfile(_:)), rep: p)
            del.isEnabled = !p.running
            sub.addItem(del)
            item.submenu = sub
            menu.addItem(item)
        }
        menu.addItem(.separator())
        menu.addItem(make("New Profile…", #selector(newProfile(_:))))
        menu.addItem(make("Import Profile…", #selector(importProfile(_:))))

        // Backup & Restore — app-level (real app data), not per-profile
        let backupItem = NSMenuItem(title: "Back Up & Restore", action: nil, keyEquivalent: "")
        let backupSub = NSMenu()
        backupSub.autoenablesItems = false
        for b in CLI.backupApps() {
            let disp = CLI.displayName(b.key)
            let appItem = NSMenuItem(title: b.verdict == "experimental" ? "\(disp)  (experimental)" : disp,
                                     action: nil, keyEquivalent: "")
            let appSub = NSMenu()
            appSub.addItem(make("Back Up…", #selector(backupApp(_:)), rep: b.key))
            appSub.addItem(make("Restore…", #selector(restoreApp(_:)), rep: b.key))
            appItem.submenu = appSub
            backupSub.addItem(appItem)
        }
        if backupSub.items.isEmpty {
            let none = NSMenuItem(title: "no backup-capable apps found", action: nil, keyEquivalent: "")
            none.isEnabled = false; backupSub.addItem(none)
        }
        backupItem.submenu = backupSub
        menu.addItem(backupItem)

        menu.addItem(make("Rescan Installed Apps", #selector(rescan(_:))))
        menu.addItem(.separator())
        let v = NSMenuItem(title: "Multiapp v0.2", action: nil, keyEquivalent: "")
        v.isEnabled = false
        if let ic = NSApp.applicationIconImage.copy() as? NSImage {
            ic.size = NSSize(width: 16, height: 16)
            v.image = ic
        }
        menu.addItem(v)
        menu.addItem(make("Quit Multiapp", #selector(NSApplication.terminate(_:)), key: "q"))
    }

    func make(_ title: String, _ sel: Selector, rep: Any? = nil, key: String = "") -> NSMenuItem {
        let i = NSMenuItem(title: title, action: sel, keyEquivalent: key)
        i.target = (sel == #selector(NSApplication.terminate(_:))) ? NSApp : self
        i.representedObject = rep
        return i
    }

    // ------------------------------------------------------------ actions
    @objc func launch(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        DispatchQueue.global().async {
            let r = CLI.run(["launch", p.app, p.name])
            if r.code != 0 { self.alertAsync("Launch failed", r.err) }
        }
    }

    @objc func stop(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        DispatchQueue.global().async {
            let r = CLI.run(["stop", p.app, p.name])
            if r.code != 0 { self.alertAsync("Stop failed", r.err) }
        }
    }

    /// Modal one-line text prompt (centered style). Returns nil on cancel/empty.
    func promptText(_ title: String, _ message: String, placeholder: String, ok: String = "OK") -> String? {
        let d = CenteredDialog(title: title, message: message)
        let field = NSTextField()
        field.placeholderString = placeholder
        d.add(field, height: 24, gap: 14)
        d.addButton(ok, code: 1, key: "\r")
        d.addButton("Cancel", code: 2, key: "\u{1b}")
        guard d.run(firstResponder: field) == 1 else { return nil }
        let v = field.stringValue.trimmingCharacters(in: .whitespaces)
        return v.isEmpty ? nil : v
    }

    @objc func renameProfile(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        guard let newName = promptText("Rename Profile", "New name for \(p.app)/\(p.name):",
                                       placeholder: "new-name", ok: "Rename") else { return }
        DispatchQueue.global().async {
            let r = CLI.run(["rename", p.app, p.name, newName])
            if r.code != 0 { self.alertAsync("Rename failed", r.err) }
        }
    }

    @objc func cloneProfile(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        guard let newName = promptText("Clone Profile",
                                       "Copy of \(p.app)/\(p.name). Note: the clone contains the same login session.",
                                       placeholder: "clone-name", ok: "Clone") else { return }
        DispatchQueue.global().async {
            let r = CLI.run(["clone", p.app, p.name, newName])
            self.alertAsync(r.code == 0 ? "Cloned" : "Clone failed", r.code == 0 ? r.out : r.err)
        }
    }

    @objc func exportProfile(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "multiapp-\(p.app)-\(p.name).tar.gz"
        panel.title = "Export \(p.app)/\(p.name)"
        panel.prompt = "Export"
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        DispatchQueue.global().async {
            // claude → all Claude Code sessions bundled; other apps → whole profile archive
            let r = CLI.run(["export", p.app, p.name, url.path, "all"])
            self.alertAsync(r.code == 0 ? "Exported" : "Export failed",
                            r.code == 0 ? "Archive contains your login/session data — keep it private.\n\n\(url.path)" : r.err)
        }
    }

    @objc func deleteProfile(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        guard let typed = promptText("Delete Profile",
                                     "This moves \(p.app)/\(p.name) to Multiapp Trash (recoverable). Type the profile name to confirm:",
                                     placeholder: p.name, ok: "Delete") else { return }
        guard typed == p.name else { return alert("Not deleted", "Name did not match.") }
        DispatchQueue.global().async {
            // the CLI's own typed confirmation is answered via stdin
            let r = CLI.run(["delete", p.app, p.name], input: typed + "\n")
            self.alertAsync(r.code == 0 ? "Moved to Multiapp Trash" : "Delete failed",
                            r.code == 0 ? "Restore by moving it back from the Trash folder; purge via CLI: multiapp trash purge" : r.err)
        }
    }

    @objc func importProfile(_ s: NSMenuItem) {
        let panel = NSOpenPanel()
        panel.title = "Import Profile Archive"
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }

        let apps = CLI.apps()
        guard !apps.isEmpty else { return alert("No apps", "Run a scan first.") }
        let d = CenteredDialog(title: "Import Into",
                               message: "Choose the app and a NEW profile name for the imported archive.")
        let popup = NSPopUpButton(frame: .zero, pullsDown: false)
        for ap in apps { popup.addItem(withTitle: ap.display); popup.lastItem?.representedObject = ap.key }
        d.add(popup, height: 26)
        let nameField = NSTextField()
        nameField.placeholderString = "profile name"
        d.add(nameField, height: 24, gap: 14)
        d.addButton("Import", code: 1, key: "\r")
        d.addButton("Cancel", code: 2, key: "\u{1b}")
        guard d.run(firstResponder: nameField) == 1,
              let key = popup.selectedItem?.representedObject as? String else { return }
        let name = nameField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return alert("No name", "Profile name cannot be empty.") }
        DispatchQueue.global().async {
            let r = CLI.run(["import", key, name, url.path])
            self.alertAsync(r.code == 0 ? "Imported" : "Import failed", r.code == 0 ? r.out : r.err)
        }
    }

    // transfer dialog state (rebuilt per invocation)
    private var tBoxes: [NSButton] = []
    private var tScroll: NSScrollView?
    private var tFrom: NSPopUpButton?


    private func buildSessionList(for source: String) {
        let sessions = CLI.sessions(of: source)
        let rowH: CGFloat = 22, width: CGFloat = 480
        let doc = FlippedView(frame: NSRect(x: 0, y: 0, width: width,
                                            height: max(rowH * CGFloat(sessions.count), rowH)))
        tBoxes = []
        if sessions.isEmpty {
            let lbl = NSTextField(labelWithString: "no sessions in '\(source)'")
            lbl.frame = NSRect(x: 8, y: 2, width: width - 16, height: rowH)
            lbl.textColor = .secondaryLabelColor
            doc.addSubview(lbl)
        }
        for (i, sess) in sessions.enumerated() {
            let cb = NSButton(checkboxWithTitle: "\(sess.date)   \(sess.title)", target: nil, action: nil)
            cb.frame = NSRect(x: 4, y: CGFloat(i) * rowH, width: width - 8, height: rowH)
            cb.tag = sess.index
            cb.lineBreakMode = .byTruncatingTail
            doc.addSubview(cb)
            tBoxes.append(cb)
        }
        tScroll?.documentView = doc
    }

    @objc private func transferSourceChanged(_ sender: NSPopUpButton) {
        buildSessionList(for: sender.titleOfSelectedItem ?? "main")
    }

    @objc func transferSessions(_ s: NSMenuItem) {
        guard let p = s.representedObject as? CLI.Profile else { return }
        // any ↔ any: main + every claude profile
        let targets = ["main"] + CLI.profiles().filter { $0.app == "claude" }.map { $0.name }

        let d = CenteredDialog(title: "Transfer Sessions",
                               message: "Copy Claude Code sessions between any profiles.\nRestart the destination profile afterwards to see them.",
                               width: 540)

        let row = NSView(frame: NSRect(x: 0, y: 0, width: d.cw, height: 26))
        let fromLbl = NSTextField(labelWithString: "From:"); fromLbl.frame = NSRect(x: 0, y: 4, width: 42, height: 20)
        let fromPopup = NSPopUpButton(frame: NSRect(x: 44, y: 0, width: 172, height: 26), pullsDown: false)
        fromPopup.addItems(withTitles: targets)
        fromPopup.selectItem(withTitle: "main")
        fromPopup.target = self
        fromPopup.action = #selector(transferSourceChanged(_:))
        tFrom = fromPopup
        let arrowLbl = NSTextField(labelWithString: "→"); arrowLbl.frame = NSRect(x: 228, y: 4, width: 20, height: 20)
        let toLbl = NSTextField(labelWithString: "To:"); toLbl.frame = NSRect(x: 262, y: 4, width: 28, height: 20)
        let toPopup = NSPopUpButton(frame: NSRect(x: 292, y: 0, width: 188, height: 26), pullsDown: false)
        toPopup.addItems(withTitles: targets)
        toPopup.selectItem(withTitle: p.name)   // default: the profile whose menu was opened
        [fromLbl, fromPopup, arrowLbl, toLbl, toPopup].forEach { row.addSubview($0) }
        d.add(row, height: 26)

        let scroll = NSScrollView(frame: .zero)
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        tScroll = scroll
        buildSessionList(for: "main")
        d.add(scroll, height: 250, gap: 14)

        d.addButton("Transfer Selected", code: 1, key: "\r")
        d.addButton("Transfer All", code: 2)
        d.addButton("Cancel", code: 3, key: "\u{1b}")

        let resp = d.run()
        defer { tScroll = nil; tFrom = nil; tBoxes = [] }
        guard resp == 1 || resp == 2 else { return }

        let from = fromPopup.titleOfSelectedItem ?? "main"
        let to = toPopup.titleOfSelectedItem ?? p.name
        guard from != to else { return alert("Same profile", "Source and destination must differ.") }

        let presel: String
        if resp == 2 {
            presel = "all"
        } else {
            let picked = tBoxes.filter { $0.state == .on }.map { String($0.tag) }
            guard !picked.isEmpty else { return alert("Nothing selected", "Tick at least one session, or use Transfer All.") }
            presel = picked.joined(separator: ",")
        }
        DispatchQueue.global().async {
            let r = CLI.run(["transfer", "claude", from, to, presel])
            self.alertAsync(r.code == 0 ? "Sessions transferred" : "Transfer failed",
                            r.code == 0 ? r.out + "\nRestart the '\(to)' profile to see them." : r.err)
        }
    }

    @objc func rescan(_ s: NSMenuItem) {
        DispatchQueue.global().async {
            let r = CLI.run(["scan"])
            // drop the CLI's progress line — the dialog only needs the result
            let cleaned = (r.out.isEmpty ? r.err : r.out)
                .split(separator: "\n")
                .filter { !$0.hasPrefix("scanning") }
                .joined(separator: "\n")
            self.alertAsync("Scan finished", cleaned)
        }
    }

    @objc func backupApp(_ s: NSMenuItem) {
        guard let key = s.representedObject as? String else { return }
        let disp = CLI.displayName(key)
        // the app must be quit for a consistent copy — offer to quit it
        if CLI.isRunning(app: key) {
            let d = CenteredDialog(title: "Quit \(disp)?",
                                   message: "\(disp) must be closed for a clean backup. Quit it now?\nSave any unsaved work first.")
            d.addButton("Quit & Back Up", code: 1, key: "\r")
            d.addButton("Cancel", code: 2, key: "\u{1b}")
            guard d.run() == 1 else { return }
            CLI.quit(app: key)
        }
        let panel = NSSavePanel()
        panel.title = "Back Up \(disp)"
        panel.nameFieldStringValue = "multiapp-backup-\(key)-\(Self.stamp()).tar.gz"
        panel.prompt = "Back Up"
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        setBusy(true, "backing up \(disp)…")
        DispatchQueue.global().async {
            let r = CLI.run(["backup", key, url.path])
            let ok = r.code == 0
            self.setBusy(false)
            self.alertAsync(ok ? "Backup complete" : "Backup failed",
                            ok ? (r.out.split(separator: "\n").first(where: { $0.contains("backup written") }).map(String.init) ?? url.path)
                               : (r.err.isEmpty ? r.out : r.err))
        }
    }

    /// Dim the menu-bar icon + set a tooltip while a long op runs (no blocking modal).
    func setBusy(_ on: Bool, _ note: String = "") {
        DispatchQueue.main.async {
            self.statusItem.button?.appearsDisabled = on
            self.statusItem.button?.toolTip = on ? "Multiapp — \(note)" : nil
        }
    }

    @objc func restoreApp(_ s: NSMenuItem) {
        guard let key = s.representedObject as? String else { return }
        let disp = CLI.displayName(key)
        let panel = NSOpenPanel()
        panel.title = "Restore \(disp) from backup"
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = []
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }

        if CLI.isRunning(app: key) { CLI.quit(app: key) }
        let d = CenteredDialog(title: "Restore \(disp)?",
                               message: "This replaces \(disp)'s current data with the backup.\nYour current data is moved to Multiapp Trash first (recoverable).")
        d.addButton("Restore", code: 1, key: "\r")
        d.addButton("Cancel", code: 2, key: "\u{1b}")
        guard d.run() == 1 else { return }
        setBusy(true, "restoring \(disp)…")
        DispatchQueue.global().async {
            // CLI asks for a typed "RESTORE" confirmation on stdin; we've confirmed in the GUI already
            let r = CLI.run(["restore", key, url.path], input: "RESTORE\n")
            let ok = r.code == 0
            self.setBusy(false)
            self.alertAsync(ok ? "Restore complete" : "Restore failed",
                            ok ? "\(disp)'s data was restored. Relaunch it — on this Mac your login is intact too."
                               : (r.err.isEmpty ? r.out : r.err))
        }
    }

    static func stamp() -> String {
        let f = DateFormatter(); f.dateFormat = "yyyyMMdd-HHmmss"
        return f.string(from: Date())
    }

    @objc func newProfile(_ s: NSMenuItem) {
        let apps = CLI.apps()
        guard !apps.isEmpty else { return alert("No launchable apps", "Run a scan first.") }

        let d = CenteredDialog(title: "New Profile",
                               message: "Choose the app and name the profile\n(letters, digits, hyphens).")
        let popup = NSPopUpButton(frame: .zero, pullsDown: false)
        for a in apps {
            popup.addItem(withTitle: a.verdict == "supported" ? a.display : "\(a.display)  (\(a.verdict))")
            popup.lastItem?.representedObject = a.key
        }
        d.add(popup, height: 26)
        let nameField = NSTextField()
        nameField.placeholderString = "profile name, e.g. work"
        d.add(nameField, height: 24, gap: 14)
        d.addButton("Create", code: 1, key: "\r")
        d.addButton("Create + Launch", code: 2)
        d.addButton("Cancel", code: 3, key: "\u{1b}")

        let clicked = d.run(firstResponder: nameField)
        guard clicked == 1 || clicked == 2 else { return }
        guard let key = popup.selectedItem?.representedObject as? String else { return }
        let name = nameField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return alert("No name", "Profile name cannot be empty.") }

        let launchToo = (clicked == 2)
        DispatchQueue.global().async {
            let r = CLI.run(["new", key, name])
            if r.code != 0 { return self.alertAsync("Create failed", r.err) }
            if launchToo {
                let l = CLI.run(["launch", key, name])
                if l.code != 0 { self.alertAsync("Created, but launch failed", l.err) }
            }
        }
    }

    // ------------------------------------------------------------ alerts (centered style)
    func alert(_ title: String, _ text: String) {
        let d = CenteredDialog(title: title, message: text.trimmingCharacters(in: .whitespacesAndNewlines))
        d.addButton("OK", code: 1, key: "\r")
        _ = d.run()
    }
    func alertAsync(_ title: String, _ text: String) {
        DispatchQueue.main.async { self.alert(title, text) }
    }
}

// ---------------------------------------------------------------- main
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)   // menu-bar only, no Dock icon
app.run()
