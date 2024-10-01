
import QtQuick 2.0
import Sailfish.Silica 1.0
import Nemo.Ngf 1.0
import Nemo.DBus 2.0
import be.rubdos.whisperfish 1.0

Dialog {
    id: incomingCallDialog

    Loader {
        id: caller
        asynchronous: true

        sourceComponent: Component{
            Recipient {
                app: AppState
                recipientId: calls.ringing
            }
        }
    }

    Connections {
        target: calls
        onHungup: {
            ringtone.stop()
            mce.setCallState(mce.callStateNone)
            pageStack.pop()
        }
    }

    readonly property string contactName: caller.loaded ? getRecipientName(caller.item.e164, caller.item.externalId, caller.item.name, false) : "..."


    // Play ringtone always via ngfd
    NonGraphicalFeedback {
        id: ringtone
        event: "voip_ringtone"
        properties: [
            NgfProperty { name: "sound.filename"; value: "/usr/share/simkit/audio/ringing-tone.wav" }
        ]
    }

    Component.onCompleted: {
        ringtone.play()
        mce.setCallState(mce.callStateRinging)
    }

    onAccepted: {
        ringtone.stop()
        // mce.setCallState(mce.callStateActive)
        mce.setCallState(mce.callStateNone)
        calls.answer()
    }

    onRejected: {
        ringtone.stop()
        mce.setCallState(mce.callStateNone)
        calls.hangup()
        pageStack.pop();
    }

    DBusInterface {
        id: mce

        // These are copied from /usr/include/mce/mode-names.h - mce-headers package
        readonly property string callStateNone: "none"
        readonly property string callStateRinging: "ringing"
        readonly property string callStateActive: "active"
        readonly property string callType: "normal"

        function setCallState(state) {
            call("req_call_state_change", [state, callType])
        }

        bus: DBus.SystemBus
        service: 'com.nokia.mce'
        path: '/com/nokia/mce/request'
        iface: 'com.nokia.mce.request'
    }


    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            id: header
            title: contactName
        }

        Label {
            text: calls.callType == 0
                   //: Title of the dialog shown when a voice call is incoming
                   //% "Incoming voice call"
                   ? qsTrId("whisperfish-incoming-voice-call-title")
                   //: Title of the dialog shown when a video call is incoming
                   //% "Incoming video call"
                   : qsTrId("whisperfish-incoming-video-call-title")
            //font.bold: true
            font.pointSize: Theme.fontSizeExtraLarge
            horizontalAlignment: Text.AlignHCenter
        }

        ButtonLayout {
            id: answerDeclineLayout
            // anchors.centerIn: parent
            width: parent.width - 2 * Theme.horizontalPageMargin
            columnSpacing: Theme.paddingLarge
            visible: opacity > 0.0
            opacity: calls.direction == 1 ? 1.0 : 0.0
            Behavior on opacity { FadeAnimation {} }

            Button {
                //: Button to decline an incoming call
                //% "Decline"
                text: qsTrId("whisperfish-calling-decline")
                color: "red"
                onClicked: {
                    incomingCallDialog.reject()
                }
            }

            Button {
                //: Button to answer an incoming call
                //% "Answer"
                text: qsTrId("whisperfish-calling-answer")
                color: "green"
                onClicked: {
                    incomingCallDialog.accept()
                }
            }
        }

        ButtonLayout {
            id: hangupLayout
            // anchors.centerIn: parent
            width: parent.width - 2 * Theme.horizontalPageMargin
            columnSpacing: Theme.paddingLarge
            visible: opacity > 0.0
            opacity: calls.direction == 0 ? 1.0 : 0.0
            Behavior on opacity { FadeAnimation {} }

            Button {
                //: Button to hang up/cancel an outgoing ringing call
                //% "Hang up"
                text: qsTrId("whisperfish-calling-hang-up")
                color: "red"
                onClicked: {
                    incomingCallDialog.reject()
                }
            }
        }
    }
}
