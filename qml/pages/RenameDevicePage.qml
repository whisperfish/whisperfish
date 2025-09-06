import QtQuick 2.2
import Sailfish.Silica 1.0

Dialog {
    property int device_id
    property string device_name

    canAccept: newName.acceptableInput

    onAccepted: {
        ClientWorker.renameLinkedDevice(device_id, newName.text);
    }

    Column {
        anchors {
            left: parent.left
            top: parent.top
            right: parent.right
        }

        DialogHeader {}

        TextField {
            id: newName

            //: Short description for rename device input field
            //% "New device name"
            label: qsTrId("whisperfish-rename-device-input-label")
            //: Description for rename device input field
            //% "Rename device \"%1\""
            description: qsTrId("whisperfish-rename-device-input-desc").arg(device_name)
            // EditDeviceNameFragment.kt -- MAX_LENGTH = 50
            acceptableInput: text.length > 0 && text.length <= 50
        }
    }
}
