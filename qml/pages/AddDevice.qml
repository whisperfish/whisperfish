import QtQuick 2.2
import Sailfish.Silica 1.0

Dialog {
    id: addDeviceDialog
    objectName: "addDeviceDialog"
    canAccept: urlField.acceptableInput && !camera.active
    readonly property bool active: Qt.application.active

    property alias camera: cameraLoader.item

    onActiveChanged: {
        if (cameraLoader.status !== Loader.Ready) {
            return;
        }

        if(active) {
            camera.stop()
        }
        else {
            camera.start()
            camera.unlock()
        }
    }

    onStatusChanged: {
        if (cameraLoader.status !== Loader.Ready) {
            return;
        }

        if(status === PageStatus.Active) {
            camera.start()
            camera.unlock()
        }
        else {
            camera.stop()
        }
    }

    signal addDevice(string tsurl)

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            acceptText: ""
            //: Add Device, shown as pull-down menu item
            //% "Add Device"
            title: qsTrId("whisperfish-add-device")
        }

        // Load "AddDeviceQrScanner.qml"
        Loader {
            id: cameraLoader
            source: "../components/AddDeviceQrScanner.qml"
            width: parent.width
            visible: camera.active

            onLoaded: {
                if (addDeviceDialog.active) {
                    camera.start()
                    camera.unlock()
                }
            }
        }

        Connections {
            // enabled: cameraLoader.status === Loader.Ready

            target: cameraLoader.item
            onResultFound: {
                addDevice(tsurl)
                addDeviceDialog.close()
            }
        }

        Label {
            width: parent.width
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            //: Instructions on how to scan QR code for device linking
            //% "Scan the QR code displayed by the Signal application that you wish to link"
            text: qsTrId("whisperfish-qr-scanning-instructions")
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor

            visible: camera != null && camera.active

            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
            }
        }

        TextField {
            id: urlField
            width: parent.width
            inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhSensitiveData | Qt.ImhNoAutoUppercase | Qt.ImhPreferLowercase
            validator: RegExpValidator{ regExp: /(tsdevice|sgnl):\/\/?.*/;}
            //: Device URL, text input for pasting the QR-scanned code
            //% "Device URL"
            label: qsTrId("whisperfish-device-url")
            placeholderText: "sgnl://[...]"
            horizontalAlignment: TextInput.AlignLeft
            EnterKey.onClicked: parent.focus = true

            visible: camera == null || !camera.active

            errorHighlight: !(urlField.text.length > 0 && urlField.acceptableInput)

            Component.onCompleted: {
                if(urlField.rightItem !== undefined) {
                    _urlFieldLoader.active = true
                    urlField.rightItem = _urlFieldLoader.item
                    urlField.errorHighlight = false
                }
            }

            Loader {
                id: _urlFieldLoader
                active: false
                sourceComponent: Image {
                    width: urlField.font.pixelSize
                    height: urlField.font.pixelSize
                    source: "image://theme/icon-s-checkmark?" + urlField.color
                    opacity: urlField.text.length > 0 && urlField.acceptableInput ? 1.0 : 0.01
                    Behavior on opacity { FadeAnimation {} }
                }
            }
        }

        Label {
            width: parent.width
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            //: Instructions on how to scan QR code for device linking
            //% "Install Signal Desktop. Use e.g. the CodeReader application to scan the QR code displayed on Signal Desktop and copy and paste the URL here."
            text: qsTrId("whisperfish-device-link-instructions")
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor

            visible: camera == null || !camera.active

            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
            }
        }
    }
}
