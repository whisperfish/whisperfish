import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Nemo.DBus 2.0
import "../components"

BlockingInfoPageBase {
    id: root
    objectName: "testCaptchaPage"

    pageTitle: "" // xx("Step 2")

    //: Captcha test page title
    //% "Captcha Test"
    mainTitle:  qsTrId("whisperfish-captcha-test-title")

    //: Captcha test page description
    //% "You can use this page to test the Whisperfish captcha challenge integration"
    mainDescription: qsTrId("whisperfish-captcha-test-message")

    busy: false
    property bool captchaReceived: false
    property string captchaToken

    // Don't lock user in, this is a test!
    backNavigation: true
    showNavigationIndicator: true

    ConfigurationValue {
        key: "/apps/harbour-whisperfish/captchaType"
        Component.onCompleted: {
            // Registration captcha doesn't require
            // additional information, so let's use that.
            value = "registration"
            sync()
        }
    }

    DBusAdaptor {
        service: "be.rubdos.harbour-whisperfish"
        path: "/be/rubdos/whisperfish/captcha"
        iface: "be.rubdos.whisperfish.captcha"

        function handleCaptcha(captchaResponse) {
            console.log("handleCaptcha()")
            if(!captchaReceived) {
                captchaReceived = true
                mainWindow.activate()
                console.debug("Submit token:", captchaToken)
                console.debug("Submit captcha:", captchaResponse)
                root.busy = false
                mainDescription = (
                    qsTrId("whisperfish-captcha-test-message") +
                    //: Captcha test successful message
                    //% "Captcha token received!"
                    "\n\n" + qsTrId("whisperfish-captcha-test-success")
                )
            }
        }
    }

    Timer {
        id: captchaTimer
        interval: 200
        running: false
        repeat: false
        onTriggered: {
            captchaReceived = false
            Prompt.startCaptcha()
        }
    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        Button {
            //: continue button label
            //% "Start"
            text: qsTrId("whisperfish-start-test-button-label")
            enabled: !root.busy
            onClicked: {
                mainDescription = (
                    qsTrId("whisperfish-captcha-test-message") +
                    //: Captcha test has been started message
                    //% "Test started..."
                    "\n\n" + qsTrId("whisperfish-captcha-test-started")
                )
                captchaTimer.restart()
                root.busy = true
            }
            anchors.horizontalCenter: parent.horizontalCenter
        }
    }
}
