import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Nemo.DBus 2.0
import "../components"

BlockingInfoPageBase {
    id: root
    objectName: "proofSubmitName"

    pageTitle: "" // xx("Step 2")

    //: Signal has requested additional captcha page title
    //% "Captcha requested"
    mainTitle: qsTrId("whisperfish-captcha-requested-title")

    //: Signal has requested additional captcha description
    //% "Signal has requested an additional captcha from you. Complete the captcha in order to restore the ability to send messages."
    mainDescription: qsTrId("whisperfish-captcha-requested-message")

    busy: false
    property bool captchaReceived: false
    property string captchaToken

    ConfigurationValue {
        key: "/apps/harbour-whisperfish/captchaType"
        Component.onCompleted: {
            value = "challenge"
            sync()
        }
    }

    Connections {
        target: ClientWorker
        // Called when the captcha has been completed and ack'd or err'd by server
        onProofCaptchaResult: {
            if (success) {
                pageStack.pop()
            } else {
                root.busy = false
                captchaReceived = false
                mainDescription = (qsTrId("whisperfish-captcha-requested-message") +
                    //: Rate limit captcha has to be tried again
                    //% "The captcha response wasn't accepted, please try again."
                    "\n\n" + qsTrId("whisperfish-captcha-requested-try-again"))
            }
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
                console.log("Submit token:", captchaToken)
                console.log("Submit captcha:",captchaResponse)
                ClientWorker.submit_proof_captcha(captchaToken, captchaResponse)
                // Busy, until proofCaptchaResult(bool) arrives.
            }
        }
    }

    Timer {
        id: captchaTimer
        interval: 100
        running: false
        repeat: false
        onTriggered: {
            captchaReceived = false
            Prompt.startCaptcha()
        }
    }

    Timer {
        id: popTimer
        interval: 100
        running: false
        repeat: false
        onTriggered: {
            pageStack.pop()
        }
    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        Button {
            text: (captchaReceived
                //: Done button label
                //% "Done"
                ? qsTrId("whisperfish-done-button-label")
                //: continue button label
                //% "Continue"
                : qsTrId("whisperfish-continue-button-label")
            )
            enabled: !root.busy
            onClicked: {
                if (captchaReceived) {
                    popTimer.restart()
                    root.busy = true
                } else {
                    captchaTimer.restart()
                    root.busy = true
                }
            }
            anchors.horizontalCenter: parent.horizontalCenter
        }
    }
}
