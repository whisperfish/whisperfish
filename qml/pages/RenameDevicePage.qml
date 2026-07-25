import QtQuick 2.2
import Sailfish.Silica 1.0

Dialog {
    id: root

    property int deviceId
    property string deviceName

    canAccept: newName.acceptableInput

    onAccepted: {
        ClientWorker.renameLinkedDevice(deviceId, newName.text);
    }

    Column {
        anchors {
            left: parent.left
            top: parent.top
            right: parent.right
        }

        DialogHeader { }

        TextField {
            id: newName

            //: Short description for rename device input field
            //% "New device name"
            label: qsTrId("whisperfish-rename-device-input-label")
            //: Description for rename device input field
            //% "Rename device \"%1\""
            description: qsTrId("whisperfish-rename-device-input-desc").arg(root.deviceName)
            // EditDeviceNameFragment.kt -- MAX_LENGTH = 50
            acceptableInput: text.length > 0 && text.length <= 50
        }
    }
}
