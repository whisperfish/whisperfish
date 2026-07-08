/*
 * Bespoke QR scanner page for username links.
 *
 * A full-page Camera + QrFilter, lifted from AddDeviceQrScanner.qml but
 * standalone (the camera/QrFilter components are trivial enough that a
 * dedicated page keeps the concerns clean). Emits `resultFound(link)` when a
 * `signal.me` username link is captured; the pushing page connects to that
 * signal to feed `CreateConversation.query`.
 *
 * Camera testing happens on-device; the `Amber.QrFilter` + `QtMultimedia`
 * stack mirrors AddDeviceQrScanner.qml exactly, so the same runtime
 * prerequisites apply.
 */

import QtQuick 2.2
import Sailfish.Silica 1.0
import Amber.QrFilter 1.0
import QtMultimedia 5.6

Page {
    id: root
    objectName: "usernameQrScannerPage"

    //: QR scanner page title
    //% "Scan username link"
    property string title: qsTrId("whisperfish-username-qr-scan-title")

    signal resultFound(string link)

    function _isUsernameLink(s) {
        // Accept full https://signal.me/#eu/... links only; the legacy
        // sgnl:// scheme is intentionally not handled (Signal-Android no
        // longer accepts it either).
        return s.indexOf("https://signal.me/#eu/") === 0
            || s.indexOf("signal.me/#eu/") >= 0
    }

    Camera {
        id: camera
        position: Camera.BackFace
        captureMode: Camera.CaptureStillImage

        exposure {
            exposureMode: Camera.ExposureAuto
        }

        onCameraStatusChanged: {
            if (cameraStatus === Camera.ActiveStatus) {
                camera.unlock()
            }
        }
    }

    VideoOutput {
        id: videoOutput
        source: camera
        fillMode: VideoOutput.PreserveAspectFit
        anchors.fill: parent

        visible: camera.cameraStatus === Camera.ActiveStatus

        filters: [ qrFilter ]

        MouseArea {
            anchors.fill: parent
            onClicked: {
                camera.unlock()
                camera.searchAndLock()
            }
        }
    }

    ViewPlaceholder {
        enabled: camera.cameraStatus !== Camera.ActiveStatus
        //: Hint shown while the camera is starting up
        //% "Starting camera…"
        text: qsTrId("whisperfish-username-qr-camera-starting")
    }

    QrFilter {
        id: qrFilter
        onResultChanged: {
            if (result.length > 0 && root._isUsernameLink(result)) {
                camera.stop()
                root.resultFound(result)
                // Pop back to the create-conversation page; it received the
                // link via the connected signal and will drive the lookup.
                pageStack.pop()
            }
        }
    }

    Component.onDestruction: {
        if (camera.cameraStatus === Camera.ActiveStatus) {
            camera.stop()
        }
    }
}
