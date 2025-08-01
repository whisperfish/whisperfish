import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"

Page {
    id: recipientProfilePage
    objectName: "recipientProfilePage"

    property string profilePicture: ""
    property QtObject session
    property QtObject recipient

    // For new message notifications
    property int sessionId: session ? session.sessionId : -1

    Component.onCompleted: recipient.fingerprintNeeded = true

    // If entering from a group setting, don't expose direct message controls
    property bool groupContext: false

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        RemorsePopup { id: remorse }

        PullDownMenu {
            MenuItem {
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-reset-identity-menu")
                visible: SettingsBridge.debug_mode
                onClicked: {
                    // Translation in ProfilePage.qml
                    remorse.execute(qsTrId("whisperfish-reset-identity-message"),
                        function() {
                            console.log("Resetting identity key: " + recipient.e164)
                            SessionModel.removeIdentities(recipient.recipientId)
                        })
                }
            }
            MenuItem {
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-reset-session-menu")
                visible: SettingsBridge.debug_mode
                onClicked: {
                    // Translation in ProfilePage.qml
                    remorse.execute(qsTrId("whisperfish-reset-session-message"),
                        function() {
                            console.log("Resetting secure session with " + recipient.e164)
                            MessageModel.endSession(recipient.recipientId)
                        })
                }
            }
            MenuItem {
                // Don't show when message request is unanswered
                visible: recipient.accepted || recipient.blocked
                text: recipient.blocked
                    //: Menu action to unblock a recipient
                    //% "Unblock"
                    ? qsTrId("whisperfish-recipient-unblock")
                    //: Menu action to block a recipient
                    //% "Block"
                    : qsTrId("whisperfish-recipient-block")
                onClicked: {
                    recipient.blocked
                        ? ClientWorker.handleMessageRequest(recipient.recipientUuid, "accept")
                        : ClientWorker.handleMessageRequest(recipient.recipientUuid, "block")
                    // XXX Workaround until recipient update propagates back
                    recipient.recipientUuid = recipient.recipientUuid
                }
            }
            MenuItem {
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-refresh-profile-menu")
                visible: SettingsBridge.debug_mode || !recipient.isRegistered
                onClicked: {
                    ClientWorker.refresh_profile(recipient.recipientId)
                }
            }
            MenuItem {
                //: Show a peer's system contact page (menu item)
                //% "Show contact"
                text: qsTrId("whisperfish-show-contact-page-menu")
                enabled: recipient && (recipient.e164 && recipient.e164[0] === '+' || recipient.externalId)
                visible: enabled
                onClicked: {
                    var contact = recipient.externalId
                        ? resolvePeopleModel.personById(parseInt(recipient.externalId))
                        : resolvePeopleModel.personByPhoneNumber(recipient.e164)
                    if (contact != null) {
                        pageStack.push(pageStack.resolveImportPage('Sailfish.Contacts.ContactCardPage'), { contact: contact })
                    } else if (recipient.e164 && recipient.e164[0] === '+') {
                        var newContact = resolvePeopleModel.createContact(recipient.e164, recipient.givenName, recipient.familyName)
                        pageStack.push(pageStack.resolveImportPage('Sailfish.Contacts.ContactCardPage'), { contact: newContact })
                    }
                }
            }
            MenuItem {
                text: recipient.externalId != null
                    //: Menu action to unlink a Signal contact from a Sailfish OS contact
                    //% "Unlink contact"
                    ? qsTrId("whisperfish-recipient-unlink")
                    //: Menu action to pick a Sailfish OS contact to link the Signal user to
                    //% "Link contact"
                    : qsTrId("whisperfish-recipient-link")
                onClicked: recipient.externalId != null
                    ? ClientWorker.unlinkRecipient(recipient.recipientId)
                    : pageStack.push(Qt.resolvedUrl("LinkContactPage.qml"), { recipient: recipient })
            }
            MenuItem {
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-save-message-expiry")
                visible: !groupContext && session != null && expiringMessages.newDuration !== session.expiringMessageTimeout
                onClicked: MessageModel.createExpiryUpdate(sessionId, expiringMessages.newDuration)
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: recipient.name
                description: recipient.about
            }

            ProfilePicture {
                enabled: imageStatus === Image.Ready
                height: 2*Theme.itemSizeLarge
                width: height
                highlighted: false
                labelsHighlighted: false
                imageSource: "file://" + SettingsBridge.avatar_dir + "/" + recipient.uuid
                isGroup: false
                showInfoMark: true
                infoMarkSource: 'image://theme/icon-s-chat'
                infoMarkSize: 0.9*Theme.iconSizeSmallPlus
                infoMarkEmoji: recipient.emoji
                anchors.horizontalCenter: parent.horizontalCenter
                onClicked: pageStack.push(Qt.resolvedUrl("ViewImagePage.qml"), { title: recipient.name, path: imageSource })
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                horizontalAlignment: Qt.AlignHCenter
                color: Theme.highlightColor
                visible: !recipient.isRegistered
                readOnly: true
                width: parent.width
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-profile-page-unregistered-profile")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                horizontalAlignment: Qt.AlignHCenter
                color: Theme.highlightColor
                visible: recipient.isRegistered && (!recipient.accepted || recipient.blocked)
                readOnly: true
                width: parent.width
                text: recipient.blocked
                    //: Recipient profile page, blocked recipient into
                    //% "You have blocked the recipient."
                    ? qsTrId("whisperfish-profile-page-blocked-recipient")
                    //: Recipient profile page, message request is pending
                    //% "You can't communicate with the recipient until you accept their message request."
                    : qsTrId("whisperfish-profile-page-message-request-pending")
            }

            Row {
                spacing: Theme.paddingMedium
                anchors.left: parent.left
                width: parent.width - Theme.horizontalPageMargin

                TextField {
                    id: profileFullName
                    readOnly: true
                    visible: text.length > 0
                    width: parent.width - voiceCallButton.width - videoCallButton.width - 2*Theme.paddingMedium
                    font.pixelSize: Theme.fontSizeLarge
                    // Translation in ProfilePage.qml
                    label: qsTrId("whisperfish-profile-joined-name")
                    text: recipient.name
                }

                IconButton {
                    id: voiceCallButton
                    visible: SetupWorker.callingSupported && SettingsBridge.debug_mode
                    anchors.verticalCenter: parent.verticalCenter
                    icon.source: "image://theme/icon-m-call"
                    onClicked: {
                        calls.call(recipient.id, false);
                    }
                }

                IconButton {
                    id: videoCallButton
                    visible: SetupWorker.callingSupported && SettingsBridge.debug_mode
                    anchors.verticalCenter: parent.verticalCenter
                    icon.source: "image://theme/icon-m-video"
                    onClicked: {
                        calls.call(recipient.id, true);
                    }
                }
            }

            TextField {
                readOnly: true
                visible: SettingsBridge.debug_mode && text.length > 0
                width: parent.width
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeMedium
                // Translation in ProfilePage.qml
                label: qsTrId("whisperfish-profile-uuid")
                text: recipient.uuid
            }


            Label {
                visible: recipient.uuid == null || recipient.uuid == "00000000-0000-0000-0000-000000000000"
                anchors {
                    left: parent.left
                    right: parent.right
                    leftMargin: Theme.paddingLarge
                    rightMargin: Theme.paddingLarge
                }
                font.pixelSize: Theme.fontSizeMedium
                horizontalAlignment: Text.alignHCenter
                wrapMode: Text.Wrap
                //: Warning about recipient UUID not existing or nil (all zeros)
                //% "This user profile is broken and can't be used."
                text: qsTrId("whisperfish-profile-uuid-invalid-warning")
                color: Theme.errorColor
            }

            TextField {
                readOnly: true
                visible: text.length > 0
                width: parent.width
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeMedium
                // Translation in ProfilePage.qml
                label: qsTrId("whisperfish-profile-phone-number")
                text: recipient.e164 != null ? recipient.e164 : ''
            }

            TextField {
                id: profileAboutEdit
                readOnly: true
                visible: text.length > 0
                width: parent.width
                font.pixelSize: Theme.fontSizeMedium
                // Translation in ProfilePage.qml
                label: qsTrId("whisperfish-profile-about")
                text: recipient.about
            }

            ExpiringMessagesComboBox {
                id: expiringMessages
                visible: !groupContext && session != null
                width: parent.width
                duration: session.expiringMessageTimeout
            }

            ComboBox {
                id: recipientUnidentifiedMode
                visible: SettingsBridge.debug_mode
                // Translation in ProfilePage.qml
                label: qsTrId("whisperfish-profile-unidentified")
                currentIndex: recipient.unidentifiedAccessMode
                enabled: false
                menu: ContextMenu {
                    MenuItem {
                        // Translation in ProfilePage.qml
                        text: qsTrId("whisperfish-unidentified-unknown")
                    }
                    MenuItem {
                        // Translation in ProfilePage.qml
                        text: qsTrId("whisperfish-unidentified-disabled")
                    }
                    MenuItem {
                        // Translation in ProfilePage.qml
                        text: qsTrId("whisperfish-unidentified-enabled")
                    }
                    MenuItem {
                        // Translation in ProfilePage.qml
                        text: qsTrId("whisperfish-unidentified-unrestricted")
                    }
                }
            }

            SectionHeader {
                //: Verify safety numbers
                //% "Verify safety numbers"
                text: qsTrId("whisperfish-verify-contact-identity-title")
            }

            Button {
                //: Show fingerprint button
                //% "Show fingerprint"
                text: qsTrId("whisperfish-show-fingerprint")
                enabled: numericFingerprint.text.length === 0
                onClicked: {
                    var recipientFingerprintString = recipient.fingerprint.toString()
                    if(recipient.fingerprint && recipientFingerprintString.length === 60) {
                        var pretty_fp = ""
                        for(var i = 1; i <= 12; ++i) {
                            pretty_fp += recipientFingerprintString.slice(5*(i-1), (5*i))
                            if(i === 4 || i === 8) {
                                pretty_fp += "\n"
                            } else if(i < 12) {
                                pretty_fp += " "
                            }
                        }
                        numericFingerprint.text = pretty_fp
                        isKyberEnabled.checked = recipient.sessionIsPostQuantum
                        isKyberEnabled.visible = true
                    }
                }
                anchors.horizontalCenter: parent.horizontalCenter
            }

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: numericFingerprint.width + 2*Theme.paddingLarge
                height: numericFingerprint.height + 2*Theme.paddingLarge
                radius: Theme.paddingLarge
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
                visible: numericFingerprint.text.length > 0
                Label {
                    id: numericFingerprint
                    anchors.centerIn: parent
                    font.family: 'monospace'
                }
            }

            TextArea {
                id: fingerprintDirections
                anchors.horizontalCenter: parent.horizontalCenter
                readOnly: true
                font.pixelSize: Theme.fontSizeSmall
                width: parent.width
                //: Numeric fingerprint instructions
                //% "If you wish to verify the security of your end-to-end encryption with %1, compare the numbers above with the numbers on their device."
                text: qsTrId("whisperfish-numeric-fingerprint-directions").arg(recipient.name)
            }

            IconTextSwitch {
                automaticCheck: false
                visible: false
                id: isKyberEnabled
                anchors.horizontalCenter: parent.horizontalCenter
                //: Profile page: whether a contact has post-quantum secure sessions
                //% "Post-quantum keys in use"
                text: qsTrId("whisperfish-profile-pq-enabled")
                //: Profile page: description for post-quantum secure sessions
                //% "If checked, this session was initialized with post-quantum secure cryptography."
                description: qsTrId("whisperfish-profile-pq-enabled-description")
                checked: recipient.sessionIsPostQuantum
                icon.source: "image://theme/icon-m-device-lock"

                onClicked: {
                    if (recipient.sessionIsPostQuantum) {
                        return;
                    }
                    //: Upgrading the session to Kyber remorse popup, past tense
                    //% "Session reset for post-quantum upgrade"
                    remorse.execute(qsTrId("whisperfish-kyber-click-explanation"),
                        function() {
                            console.log("Resetting secure session (pq upgrade) with " + recipient.e164)
                            MessageModel.endSession(recipient.recipientId)
                        })
                }
            }
        }
    }
}
