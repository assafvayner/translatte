package co.huggingface.translatte.ui.camera

import androidx.activity.ComponentActivity
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.ui.settings.UiPrefs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf

@RunWith(AndroidJUnit4::class)
class CameraAccessTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var opened = 0

    private fun show(asked: Boolean) {
        val prefs = UiPrefs(compose.activity).apply { cameraAsked = asked }
        compose.setContent {
            CameraAccess(prefs, onOpen = { opened++ }) { onCamera -> Button(onClick = onCamera) { Text("Camera") } }
        }
    }

    @Test
    fun afterADenialTheButtonExplainsWithoutPromptingAgain() {
        show(asked = true)
        compose.onNodeWithText("Camera").performClick()

        compose.onNodeWithText(
            "translatte needs the camera to photograph text. Allow it in Settings, or choose a photo instead.",
        ).assertExists()
        compose.onNodeWithText("Open Settings").assertExists()
        assertNull(shadowOf(compose.activity).lastRequestedPermission)
        assertEquals(0, opened)
    }

    @Test
    fun theFirstTapAsksForThePermission() {
        show(asked = false)
        compose.onNodeWithText("Camera").performClick()

        assertEquals(listOf("android.permission.CAMERA"), shadowOf(compose.activity).lastRequestedPermission.requestedPermissions.toList())
        compose.onNodeWithText("Open Settings").assertDoesNotExist()
    }

    @Test
    fun aGrantedPermissionOpensTheCamera() {
        shadowOf(compose.activity.application).grantPermissions("android.permission.CAMERA")
        show(asked = true)
        compose.onNodeWithText("Camera").performClick()
        assertEquals(1, opened)
    }
}
