import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.DBus 2.0
import "../components"
import "../js/countries.js" as CallingCodes

BlockingInfoPageBase {
    id: root
    objectName: "setupRegistrationTypePage"

    pageTitle: "" // xx("Step 1.1")

    //: registration page title
    //% "Register"
    mainTitle: qsTrId("whisperfish-registration-title")

    //: registration type prompt text
    //% "Do you want to register Whisperfish as primariy device or link it as secondary device to an existing Signal app?"
    mainDescription: qsTrId("whisperfish-registration-type-message")

    signal registerPrimary
    onRegisterPrimary: {
        busy = true // we have to wait for the backend to create qr code
        Prompt.registerAsPrimary(true)
    }

    signal registerSecondary
    onRegisterSecondary: {
        busy = true // we have to wait for the backend to create qr code
        Prompt.registerAsPrimary(false)
    }

    Connections {
        // We wait till the backend calls to continue.
        target: Prompt
        onPromptPhoneNumber: pageStack.replace(Qt.resolvedUrl("RegisterPage.qml"),
                             PageStackAction.Animated)
        onShowLinkQR: pageStack.replace(Qt.resolvedUrl("RegisterSecondaryPage.qml"),
                             PageStackAction.Animated)

    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        Button {
            //: register as primary device button label
            //% "Primary device"
            text: qsTrId("whisperfish-register-primary-button-label")
            enabled: !busy
            onClicked: registerPrimary()
            anchors.horizontalCenter: parent.horizontalCenter
        }

        Button {
            //: link as secondary device button label
            //% "Secondary device"
            text: qsTrId("whisperfish-register-secondary-button-label")
            enabled: !busy
            onClicked: registerSecondary()
            anchors.horizontalCenter: parent.horizontalCenter
        }
    }
}
