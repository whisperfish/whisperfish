import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Nemo.DBus 2.0
import "../components"
import "../js/countries.js" as CallingCodes

BlockingInfoPageBase {
    id: root
    objectName: "registerPage"

    pageTitle: "" // xx("Step 2")

    //: registration page title
    //% "Register"
    mainTitle: qsTrId("whisperfish-registration-title")

    //: registration prompt text
    //% "Enter the phone number you want to register with Signal."
    mainDescription: qsTrId("whisperfish-registration-message")

    property bool _canAccept: numberField.acceptableInput &&
                              prefixCombo.currentIndex >= 0 &&
                              numberField.text.length > 4 &&
                              numberField.text.replace(/[- ]*/, '').trim() !== ''
    property bool captchaReceived: false

    ConfigurationValue {
        key: "/apps/harbour-whisperfish/captchaType"
        Component.onCompleted: {
            value = "registration"
            sync()
        }
    }

    signal accept
    onAccept: {
        if (!_canAccept) return
        busy = true // we have to wait for the backend to prompt the next step
        var iso = prefixCombo.currentItem.iso
        SettingsBridge.country_code = iso
        if (iso === "") console.warn("registering without ISO country code")
        Prompt.phoneNumber(prefixCombo.currentItem.prefix+numberField.text)
    }

    signal _retry
    on_Retry: {
        // TODO give haptic feedback

        //: new registration prompt text asking to retry
        //% "Please retry with a valid phone number."
        mainDescription = qsTrId("whisperfish-registration-retry-message")
        busy = false
    }

    Connections {
        // We wait till the backend calls to continue.
        target: Prompt
        onPromptVerificationCode: pageStack.push(Qt.resolvedUrl("VerifyRegistrationPage.qml"))
        onPromptCaptcha: captchaTimer.restart()
        onPromptPhoneNumber: _retry()
    }

    Connections {
        target: SetupWorker
        onInvalidPhoneNumber: {
            console.log("invalid phone number")
            _retry()
        }
    }

    DBusAdaptor {
        service: "be.rubdos.harbour-whisperfish"
        path: "/be/rubdos/whisperfish/captcha"
        iface: "be.rubdos.whisperfish.captcha"

        function handleCaptcha(code) {
            console.log("Received captcha:",code)
            if(!captchaReceived) {
                captchaReceived = true
                Prompt.captcha(code)
                activate()
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

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        Item {
            width: parent.width
            height: childrenRect.height

            ComboBox {
                id: prefixCombo
                width: parent.width
                enabled: !busy

                //: Label for country selection menu
                //% "Country or area"
                label: qsTrId("whisperfish-registration-country-or-area")
                currentIndex: -1
                value: currentIndex < 0
                             //: Placeholder for country not selected
                             //% "Not selected"
                             ? qsTrId("whisperfish-not-selected")
                             : currentItem.name + (currentItem.iso ? " (%1)".arg(currentItem.iso) : "")

                menu: ContextMenu {
                    Repeater {
                        model: CallingCodes.c
                        MenuItem {
                            property string prefix: CallingCodes.c[index].p
                            property string name: CallingCodes.c[index].n
                            property string iso: CallingCodes.c[index].i
                            text: name
                                 + (iso ? " (%1) ".arg(iso) : " ")
                                 + " (%1) ".arg(prefix)
                        }
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: numberField.height

            Label {
                id: countryCodeField
                opacity: !busy ? 1.0 : Theme.opacityLow
                anchors {
                    top: parent.top
                    topMargin: Theme.paddingSmall
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.paddingSmall
                }
                text: prefixCombo.currentIndex < 0
                    ? "+xx"
                    : prefixCombo.currentItem.prefix
            }

            TextField {
                id: numberField
                enabled: !busy
                anchors {
                    left: countryCodeField.right
                    right: parent.right
                    top: parent.top
                }
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhDialableCharactersOnly | Qt.ImhSensitiveData
                validator: RegExpValidator{ regExp: /|[1-9][- 0-9]{3,}/ }

                //: phone number input label
                //% "Phone number"
                label: qsTrId("whisperfish-registration-number-input-label")

                //: phone number input placeholder
                //% "Phone number"
                placeholderText: qsTrId("whisperfish-registration-number-input-placeholder")
                EnterKey.iconSource: _canAccept ?
                                            "image://theme/icon-m-enter-next" :
                                            "image://theme/icon-m-enter-close"
                EnterKey.onClicked: parent.forceActiveFocus()

                errorHighlight: !_canAccept

                Component.onCompleted: {
                    if(numberField.rightItem !== undefined) {
                        _numberFieldLoader.active = true
                        numberField.rightItem = _numberFieldLoader.item
                        numberField.errorHighlight = false
                    }
                }

                Loader {
                    id: _numberFieldLoader
                    active: false
                    sourceComponent: Image {
                        width: numberField.font.pixelSize
                        height: numberField.font.pixelSize
                        source: "image://theme/icon-s-checkmark?" + numberField.color
                        opacity: _canAccept ? 1.0 : 0.01
                        Behavior on opacity { FadeAnimation {} }
                    }
                }
            }
        }

        ComboBox {
            width: parent.width
            enabled: !busy

            //: Verification method
            //% "Verification method"
            label: qsTrId("whisperfish-verification-method-label")

            //: Registration directions
            description: SetupWorker.useVoice
                //% "Signal will call you with a 6-digit verification code. Please be ready to write it down."
                ? qsTrId("whisperfish-voice-registration-directions")
                //% "Signal will text you a 6-digit verification code."
                : qsTrId("whisperfish-text-registration-directions")

            menu: ContextMenu {
                MenuItem {
                    //: Text verification
                    //% "Use text verification"
                    text: qsTrId("whisperfish-use-text-verification")
                }
                MenuItem {
                    //: Voice verification
                    //% "Use voice verification"
                    text: qsTrId("whisperfish-use-voice-verification")
                }
            }

            onCurrentIndexChanged: {
                SetupWorker.useVoice = (currentIndex == 1)
            }
        }

        IconTextSwitch {
            id: shareContacts
            enabled: !root.busy
            visible: false // XXX: Unimplemented
            //: Share contacts label
            //% "Share Contacts"
            text: qsTrId("whisperfish-share-contacts-label")
            //: Share contacts description
            //% "Allow Signal to use your local contact list, to find other Signal users."
            description: qsTrId("whisperfish-share-contacts-description")
            checked: SettingsBridge.share_contacts
            icon.source: "image://theme/icon-m-file-vcard"
            onCheckedChanged: {
                if (checked !== SettingsBridge.share_contacts) {
                    SettingsBridge.share_contacts = checked
                }
            }
        }

        Button {
            //: continue button label
            //% "Continue"
            text: qsTrId("whisperfish-continue-button-label")
            enabled: _canAccept && !busy
            onClicked: accept()
            anchors.horizontalCenter: parent.horizontalCenter
        }
    }
}
