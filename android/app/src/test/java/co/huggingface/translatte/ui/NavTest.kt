package co.huggingface.translatte.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NavTest {
    @Test
    fun launchesIntoTheCameraWhenAllowedOrNotYetAsked() {
        assertEquals(Screen.Camera, Nav.landing(hasCamera = true, cameraGranted = true, cameraAsked = true))
        assertEquals(Screen.Camera, Nav.landing(hasCamera = true, cameraGranted = false, cameraAsked = false))
    }

    @Test
    fun deniedPermissionLandsOnHome() {
        assertEquals(Screen.Home, Nav.landing(hasCamera = true, cameraGranted = false, cameraAsked = true))
    }

    @Test
    fun noCameraLandsOnHome() {
        assertEquals(Screen.Home, Nav.landing(hasCamera = false, cameraGranted = false, cameraAsked = false))
    }

    @Test
    fun backRoutes() {
        assertEquals(Screen.Home, Nav.back(Screen.Camera, resultOrigin = Screen.Home))
        assertNull(Nav.back(Screen.Home, resultOrigin = Screen.Home))
        assertEquals(Screen.Camera, Nav.back(Screen.Result, resultOrigin = Screen.Camera))
        assertEquals(Screen.Home, Nav.back(Screen.Result, resultOrigin = Screen.Home))
        assertEquals(Screen.Home, Nav.back(Screen.Languages, resultOrigin = Screen.Camera))
        assertEquals(Screen.Home, Nav.back(Screen.Settings, resultOrigin = Screen.Camera))
    }
}
