import AVFoundation
import XCTest
@testable import Translatte

@MainActor
final class AppNavigatorTests: XCTestCase {
    func testLandingScreen() {
        XCTAssertEqual(AppNavigator.landing(modelsReady: false, camera: .authorized), .setup)
        XCTAssertEqual(AppNavigator.landing(modelsReady: true, camera: .authorized), .camera)
        XCTAssertEqual(AppNavigator.landing(modelsReady: true, camera: .notDetermined), .camera)
        XCTAssertEqual(AppNavigator.landing(modelsReady: true, camera: .denied), .home)
        XCTAssertEqual(AppNavigator.landing(modelsReady: true, camera: .unavailable), .home)
    }

    func testCameraAccessFromDeviceAndStatus() {
        XCTAssertEqual(CameraAccess.current(hasCamera: false, status: .authorized), .unavailable)
        XCTAssertEqual(CameraAccess.current(hasCamera: true, status: .authorized), .authorized)
        XCTAssertEqual(CameraAccess.current(hasCamera: true, status: .notDetermined), .notDetermined)
        XCTAssertEqual(CameraAccess.current(hasCamera: true, status: .denied), .denied)
        XCTAssertEqual(CameraAccess.current(hasCamera: true, status: .restricted), .denied)
    }

    func testResultGoesBackToWhereThePhotoCameFrom() {
        let nav = AppNavigator(modelsReady: true, cameraAccess: .authorized)
        XCTAssertEqual(nav.screen, .camera)
        nav.showResult(from: .camera)
        XCTAssertEqual(nav.screen, .result(from: .camera))
        nav.closeResult()
        XCTAssertEqual(nav.screen, .camera)
        nav.closeCamera()
        XCTAssertEqual(nav.screen, .home)
        nav.showResult(from: .home)
        nav.closeResult()
        XCTAssertEqual(nav.screen, .home)
        nav.openCamera()
        XCTAssertEqual(nav.screen, .camera)
    }

    func testDeniedPermissionLeavesTheCameraForHome() {
        let nav = AppNavigator(modelsReady: true, cameraAccess: .notDetermined)
        XCTAssertEqual(nav.screen, .camera)
        nav.cameraAccessChanged(.denied)
        XCTAssertEqual(nav.screen, .home)
        nav.openCamera()
        XCTAssertEqual(nav.screen, .home, "Home explains how to turn the camera on instead")
    }

    func testSetupComesFirstAndReturnsWhenModelsGo() {
        let nav = AppNavigator(modelsReady: false, cameraAccess: .unavailable)
        XCTAssertEqual(nav.screen, .setup)
        nav.modelsChanged(ready: true)
        XCTAssertEqual(nav.screen, .home)
        nav.modelsChanged(ready: false)
        XCTAssertEqual(nav.screen, .setup)
    }

    func testModelsReloadingDoesNotMoveAnOpenScreen() {
        let nav = AppNavigator(modelsReady: true, cameraAccess: .unavailable)
        nav.showResult(from: .home)
        nav.modelsChanged(ready: true)
        XCTAssertEqual(nav.screen, .result(from: .home))
    }
}
