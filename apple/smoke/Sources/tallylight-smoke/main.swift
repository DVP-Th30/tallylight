// Essai de bout en bout des liaisons Swift : appaire un écran avec un démon d'essai et attend la
// session annoncée. Usage : tallylight-smoke <dossier de données> <chaîne d'appairage> <session>
import Foundation
// Pour iOS, `apple/smoke-test.sh` compile ce fichier avec les liaisons, dans un seul module
#if canImport(TallylightKit)
import TallylightKit
#endif

/// Reçoit les machines sur un fil de la bibliothèque et réveille le programme à la session attendue.
final class Waiter: Listener, @unchecked Sendable {
    let expected: String
    let found = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var done = false

    init(expected: String) {
        self.expected = expected
    }

    func onMachine(machine: Machine) {
        print("machine \"\(machine.name)\" link=\(machine.link) sessions=\(machine.sessions.count)")
        for session in machine.sessions {
            print("  \(session.id) agent=\(session.agent) project=\(session.project) state=\(session.state)")
        }
        guard machine.sessions.contains(where: { $0.id == expected && $0.state == "working" }) else {
            return
        }
        lock.lock()
        defer { lock.unlock() }
        if !done {
            done = true
            found.signal()
        }
    }

    func onMachineRemoved(id: String) {
        print("machine removed: \(id)")
    }
}

let arguments = CommandLine.arguments
guard arguments.count == 4 else {
    FileHandle.standardError.write(Data("usage: tallylight-smoke <data dir> <pairing string> <session id>\n".utf8))
    exit(2)
}
// Sans tampon : la sortie reste lisible si le programme est tué pour dépassement de délai
setvbuf(stdout, nil, _IONBF, 0)

let tally = Tallylight(dataDir: arguments[1], name: "Swift smoke test")
precondition(tally.relay() == "n0", "unexpected default relay")
precondition(tally.machines() == 0, "the data directory is not empty")
// Avant `start`, l'appairage est refusé
precondition(!tally.pair(pairing: arguments[2]), "pairing was accepted before start")

let waiter = Waiter(expected: arguments[3])
tally.start(listener: waiter)
precondition(!tally.pair(pairing: "not a pairing string"), "a malformed pairing string was accepted")
guard tally.pair(pairing: arguments[2]) else {
    print("the pairing string was refused")
    exit(1)
}

let received = waiter.found.wait(timeout: .now() + 60) == .success
let machines = tally.machines()
tally.stop()
guard received else {
    print("no machine announced session \(arguments[3]) within 60 seconds")
    exit(1)
}
guard machines == 1 else {
    print("expected 1 saved machine, found \(machines)")
    exit(1)
}
print("swift smoke test passed")
