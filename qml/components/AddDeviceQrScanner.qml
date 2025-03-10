import QtQuick 2.2
import Sailfish.Silica 1.0
import Amber.QrFilter 1.0
import QtMultimedia 5.6

VideoOutput {
    id: videoOutput
    source: camera
    fillMode: VideoOutput.PreserveAspectFit
    z: -1
    width: parent.width - Theme.paddingLarge * 2
    height: parent.width - Theme.paddingLarge * 2
    anchors.horizontalCenter: parent.horizontalCenter

    visible: camera.status === Camera.ActiveStatus

    filters: [ qrFilter ]

    MouseArea {
        anchors.fill: parent
        onClicked: {
            camera.unlock()
            camera.searchAndLock()
        }
    }

    function start() {
        camera.start()
    }

    function stop() {
        camera.stop()
    }

    function unlock() {
        camera.unlock()
    }

    property alias status: camera.cameraStatus
    property bool active: camera.status === Camera.ActiveStatus

    signal resultFound(string tsurl)

    QrFilter {
        id: qrFilter
        onResultChanged: {
            if (result.length > 0 &&
                (result.indexOf("tsdevice:") == 0 || result.indexOf("sgnl:") == 0)) {
                resultFound(result)
            }
        }
    }

    Camera {
        id: camera
        position: Camera.BackFace
        captureMode: Camera.CaptureStillImage

        exposure {
            exposureMode: Camera.ExposureAuto
        }

        flash.mode: Camera.FlashOff

        onCameraStatusChanged: {
            if (cameraStatus === Camera.ActiveStatus) {
                var resolutions = camera.supportedViewfinderResolutions()
                var selectedResolution
                if (resolutions.length > 0) {
                    for (var i = 0; i < resolutions.length; i++) {
                        var resolution = resolutions[i]
                        // Looking for the largest square that will fit the width
                        if (resolution.height === resolution.width && resolution.width  <= Screen.width) {
                            selectedResolution = resolution
                        }
                    }
                }
                if (selectedResolution) {
                    camera.viewfinder.resolution = Qt.size(selectedResolution.width, selectedResolution.height)
                }
            }
        }
    }
}
