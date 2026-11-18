import CoreBluetooth
import Foundation

let prefix = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "YpsoPump_"
let baseService = CBUUID(string: "fb349b5f-8000-0080-0010-0000adde0000")
let versionCharacteristic = CBUUID(string: "669a0c20-0008-969e-e211-fcbee23b7bc5")

final class Check: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    var central: CBCentralManager!
    var pump: CBPeripheral?

    override init() {
        super.init()
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard central.state == .poweredOn else { return }
        print("scanning for \(prefix)*")
        central.scanForPeripherals(withServices: nil)
    }

    func centralManager(
        _ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any], rssi: NSNumber
    ) {
        let name = advertisementData[CBAdvertisementDataLocalNameKey] as? String ?? peripheral.name
        guard let name, name.hasPrefix(prefix) else { return }
        let services = advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? []
        print("found \(name) rssi \(rssi) services \(services)")
        central.stopScan()
        pump = peripheral
        peripheral.delegate = self
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        print("connected")
        peripheral.discoverServices(nil)
    }

    func centralManager(
        _ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?
    ) {
        fail("connect failed: \(String(describing: error))")
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        let found = peripheral.services ?? []
        print("services \(found.map(\.uuid))")
        guard let base = found.first(where: { $0.uuid == baseService }) else {
            return fail("pump base service missing")
        }
        peripheral.discoverCharacteristics([versionCharacteristic], for: base)
    }

    func peripheral(
        _ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?
    ) {
        guard let characteristic = service.characteristics?.first else {
            return fail("version characteristic missing")
        }
        peripheral.readValue(for: characteristic)
    }

    func peripheral(
        _ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        if let error { return fail("read failed: \(error)") }
        let value = characteristic.value.flatMap { String(data: $0, encoding: .utf8) } ?? "?"
        print("pump base service version: \(value)")
        exit(0)
    }

    func fail(_ message: String) {
        print(message)
        exit(1)
    }
}

let check = Check()
DispatchQueue.main.asyncAfter(deadline: .now() + 30) { check.fail("timeout") }
RunLoop.main.run()
