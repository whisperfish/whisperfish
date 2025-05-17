import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"

Page {
    id: profilePage
    objectName: "profilePage"

    property var session: null
    property string profilePicture: ""

    property bool editingProfile: false

    // If entering from a group setting, don't expose direct message controls
    property bool groupContext: false

    // For new message notifications
    property alias sessionId: session.sessionId

    onStatusChanged: {
        if (editingProfile && status === PageStatus.Inactive) {
            cancelEditing()
        }
    }

    Recipient {
        id: recipient
        app: AppState
        recipientUuid: SetupWorker.uuid
    }

    // Enter edit mode, or save changes
    function toggleEditing() {
        if (editingProfile) {
            profileGivenNameEdit.focus = false
            profileFamilyNameEdit.focus = false
            profileAboutEdit.focus = false
            profileEmojiEdit.focus = false
            if(
                profileFamilyNameEdit.text !== recipient.familyName ||
                profileGivenNameEdit.text !== recipient.givenName ||
                profileAboutEdit.text !== recipient.about ||
                profileEmojiEdit.text !== recipient.emoji
            ) {
                profileFullName.text = profileGivenNameEdit.text + " " + profileFamilyNameEdit.text
                ClientWorker.upload_profile(
                    profileGivenNameEdit.text,
                    profileFamilyNameEdit.text,
                    profileAboutEdit.text,
                    profileEmojiEdit.text
                )
            }
        }
        editingProfile = !editingProfile
    }

    // Revert changes and exit editing mode
    function cancelEditing() {
        profileFullName.text = recipient.name
        profileFamilyNameEdit.text = recipient.familyName
        profileGivenNameEdit.text = recipient.givenName
        profileAboutEdit.text = recipient.about
        profileEmojiEdit.text = recipient.emoji

        profileGivenNameEdit.focus = false
        profileFamilyNameEdit.focus = false
        profileAboutEdit.focus = false
        profileEmojiEdit.focus = false

        editingProfile = false
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        RemorsePopup { id: remorse }

        PullDownMenu {
            MenuItem {
                //: Reset identity key menu item
                //% "Reset identity key"
                text: qsTrId("whisperfish-reset-identity-menu")
                visible: SettingsBridge.debug_mode
                onClicked: {
                    //: Reset identity key remorse message (past tense)
                    //% "Identity key reset"
                    remorse.execute(qsTrId("whisperfish-reset-identity-message"),
                        function() {
                            console.log("Resetting identity key: " + recipient.e164)
                            SessionModel.removeIdentities(recipient.recipientId)
                        })
                }
            }
            MenuItem {
                //: Reset secure session menu item
                //% "Reset Secure Session"
                text: qsTrId("whisperfish-reset-session-menu")
                visible: SettingsBridge.debug_mode
                onClicked: {
                    //: Reset secure session remorse message (past tense)
                    //% "Secure session reset"
                    remorse.execute(qsTrId("whisperfish-reset-session-message"),
                        function() {
                            console.log("Resetting secure session with " + recipient.e164)
                            MessageModel.endSession(recipient.recipientId)
                        })
                }
            }
            MenuItem {
                //: Refresh contact profile menu item
                //% "Refresh Signal profile"
                text: qsTrId("whisperfish-refresh-profile-menu")
                visible: SettingsBridge.debug_mode
                onClicked: {
                    ClientWorker.refresh_profile(recipient.recipientId)
                }
            }
            MenuItem {
                //: Undo changes and exit editing you profile details menu item
                //% "Discard changes"
                text: qsTrId("whisperfish-revert-profile-changes-menu")
                enabled: editingProfile
                visible: enabled
                onClicked: cancelEditing()
            }
            MenuItem {
                text: editingProfile
                //: Save changes to your profile menu item
                //% "Save profile changes"
                ? qsTrId("whisperfish-save-profile-menu")
                //: Edit your own profile menu item
                //% "Edit profile"
                : qsTrId("whisperfish-edit-profile-menu")
                enabled: (!editingProfile || profileGivenNameEdit.acceptableInput && profileEmojiEdit.acceptableInput)
                onClicked: toggleEditing()
            }
            MenuItem {
                //: Save the new value of expiring messages timeout
                //% "Set message expiry"
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
                // TODO Implement a new page derived from ViewImagePage for showing
                //      profile pictures. A new action overlay at the bottom can provide
                //      options to change or delete the profile picture.
                //      Note: adding a PullDownMenu would be best but is not possible.
                //      ViewImagePage relies on Flickable and breaks if used with SilicaFlickable,
                //      but PullDownMenu requires a SilicaFlickable as parent.
                onClicked: pageStack.push(Qt.resolvedUrl("ViewImagePage.qml"), { title: recipient.name, path: imageSource })
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                horizontalAlignment: Qt.AlignHCenter
                color: Theme.highlightColor
                visible: !recipient.isRegistered
                readOnly: true
                width: parent.width
                //: Profile page, user is not registered warning
                //% "The recipient is not currently registered to Signal, so sending and receiving messages is not possible."
                text: qsTrId("whisperfish-profile-page-unregistered-profile")
            }

            // When not editing, display full/joined name
            TextField {
                id: profileFullName
                readOnly: true
                visible: !editingProfile && text.length > 0
                width: parent.width
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeLarge
                //: Profile, name field (first name + last name)
                //% "Name"
                label: qsTrId("whisperfish-profile-joined-name")
                text: recipient.name
            }

            // When editing, display first name field
            TextField {
                id: profileGivenNameEdit
                visible: editingProfile
                width: parent.width
                readOnly: !editingProfile
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeLarge
                //: Profile, first (given) name field, required
                //% "First name (required)"
                label: qsTrId("whisperfish-profile-given-name")
                text: recipient.givenName
                // Predictive text messes up regex as-you-type,
                // so don't use it for firstname.
                validator: RegExpValidator{ regExp: /.+/ }
                inputMethodHints: Qt.ImhNoPredictiveText
            }

            // When editing, display last name field
            TextField {
                id: profileFamilyNameEdit
                visible: editingProfile
                width: parent.width
                readOnly: !editingProfile
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeLarge
                //: Profile, last (family) name field, optional
                //% "Last name (optional)"
                label: qsTrId("whisperfish-profile-family-name")
                text: recipient.familyName != null ? recipient.familyName : ''
                // Disable prediction on lastname too for consistency.
                inputMethodHints: Qt.ImhNoPredictiveText
            }

            TextField {
                readOnly: true
                visible: SettingsBridge.debug_mode && !editingProfile && text.length > 0
                width: parent.width
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeMedium
                //: Profile UUID field
                //% "UUID"
                label: qsTrId("whisperfish-profile-uuid")
                text: recipient.uuid
            }

            TextField {
                readOnly: true
                visible: !editingProfile && text.length > 0
                width: parent.width
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeMedium
                //: Profile phone number field
                //% "Phone number"
                label: qsTrId("whisperfish-profile-phone-number")
                text: recipient.e164 != null ? recipient.e164 : ''
            }

            TextField {
                id: profileAboutEdit
                visible: editingProfile || text.length > 0
                width: parent.width
                readOnly: !editingProfile
                font.pixelSize: Theme.fontSizeMedium
                //: Profile, about you (greeting/status) field
                //% "Write something about yourself"
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
                //: Profile, sealed sending mode option
                //% "Sealed sending mode"
                label: qsTrId("whisperfish-profile-unidentified")
                currentIndex: recipient.unidentifiedAccessMode
                enabled: false
                menu: ContextMenu {
                    MenuItem {
                        //: Profile, sealed sending mode, unknown option
                        //% "Unknown"
                        text: qsTrId("whisperfish-unidentified-unknown")
                    }
                    MenuItem {
                        //: Profile, sealed sending mode, disabled option
                        //% "Disabled"
                        text: qsTrId("whisperfish-unidentified-disabled")
                    }
                    MenuItem {
                        //: Profile, sealed sending mode, enabled option
                        //% "Enabled"
                        text: qsTrId("whisperfish-unidentified-enabled")
                    }
                    MenuItem {
                        //: Profile, sealed sending mode, unrestricted option
                        //% "Unrestricted"
                        text: qsTrId("whisperfish-unidentified-unrestricted")
                    }
                }
            }

            TextField {
                id: profileEmojiEdit
                // XXX: Validate emoji character somehow
                // visible: editingProfile || text.length > 0
                visible: false
                width: parent.width
                readOnly: !editingProfile
                font.pixelSize: Theme.fontSizeMedium
                //: Profile, emoji symbol field
                //% "A few words about yourself"
                label: qsTrId("whisperfish-profile-emoji")
                text: recipient.emoji
            }

            Separator {
                horizontalAlignment: Qt.AlignHCenter
                color: Theme.highlightBackgroundColor
                width: parent.width
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                readOnly: true
                width: parent.width
                //: Signal Profile description / help text
                //% "Your profile is encrypted. Your profile and changes to it will be visible to your contacts and when you start or accept new chats."
                text: qsTrId("whisperfish-own-profile-help-text")
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Button to open link to Signal help page about profiles
                //% "Learn more"
                text: qsTrId("whisperfish-own-profile-learn-more-button")
                onClicked: Qt.openUrlExternally("https://support.signal.org/hc/articles/360007459591")
            }
        }
    }
}
