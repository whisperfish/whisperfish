import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0

SilicaListView {
    id: root
    property QtObject group
    property bool youAreAdmin

    section {
        property: 'role'
        delegate: SectionHeader {
            height: Theme.itemSizeExtraSmall
            // 2 = admin
            // 1 = user
            text: section == 2 ?
            //: Group member section label for administrator level user
            //% "Administrator"
            qsTrId("whisperfish-group-member-admin") :
            //: Group member section label for regular level user
            //% "Member"
            qsTrId("whisperfish-group-member-regular")
        }
    }

    model: group.members
    delegate: ListItem {
        id: item
        contentHeight: Theme.itemSizeMedium
        anchors {
            left: parent.left
            right: parent.right
        }

        //property bool isVerified: false // TODO implement in backend;  model.isVerified
        property bool isSelf: recipient.recipientUuid == SetupWorker.uuid
        property string profilePicture: getRecipientAvatar(recipient.e164, recipient.uuid, recipient.externalId)
        property string name: getRecipientName(recipient.e164, recipient.externalId, recipient.name, false)
        property bool isUnknownContact: name.length == 0

        onClicked: {
            if (recipient.uuid === SetupWorker.uuid) {
                pageStack.push(Qt.resolvedUrl("../pages/ProfilePage.qml"), {
                    groupContext: true
                });
            } else {
                pageStack.push(Qt.resolvedUrl("../pages/RecipientProfilePage.qml"), {
                    recipient: recipient,
                    groupContext: true
                });
            }
        }

        // For when we need the augmented fields
        Recipient {
            id: recipient
            recipientUuid: model.uuid
            app: AppState
        }

        Component.onCompleted: {
            if (isSelf && role === 2) {
                root.youAreAdmin = true;
            }
        }

        menu: Component {
            ContextMenu {
                MenuItem {
                    text: isSelf ?
                    //: Menu item to open the conversation with oneself
                    //% "Open Note to Self"
                    qsTrId("whisperfish-group-member-menu-open-note-to-self") :
                    //: Menu item to open the private chat with a group member
                    //% "Message to %1"
                    qsTrId("whisperfish-group-member-menu-direct-message").arg(isUnknownContact ? (recipient.e164 ? recipient.e164 : recipient.uuid) : name)
                    onClicked: {
                        var main = pageStack.find(function (page) {
                            return page.objectName == "mainPage";
                        });
                        pageStack.replaceAbove(main, Qt.resolvedUrl("../pages/ConversationPage.qml"), {
                            sessionId: recipient.directMessageSessionId
                        });
                    }
                    visible: recipient.directMessageSessionId != -1
                }
                MenuItem {
                    //: Menu item to start a new private chat with a group member
                    //% "Start conversation with %1"
                    text: qsTrId("whisperfish-group-member-menu-new-direct-message").arg(isUnknownContact ? (recipient.e164 ? recipient.e164 : recipient.uuid) : name)
                    onClicked: {
                        var main = pageStack.find(function (page) {
                            return page.objectName == "mainPage";
                        });
                        pageStack.replaceAbove(main, Qt.resolvedUrl("../pages/CreateConversationPage.qml"), {
                            uuid: recipient.uuid
                        });
                    }
                    visible: recipient.directMessageSessionId == -1 && !isSelf
                    enabled: recipient.uuid != ""
                }
                MenuItem {
                    //: Menu item to save a group member to the local address book
                    //% "Add to contacts"
                    text: qsTrId("whisperfish-group-member-menu-save-contact")
                    visible: isUnknownContact
                    onClicked: item.clicked(null) // show contact page
                }
                MenuItem {
                    //: Menu item to remove a member from a group (requires admin privileges)
                    //% "Remove from this group"
                    text: qsTrId("whisperfish-group-member-menu-remove-from-group")
                    visible: isSelf && role === 2
                    onClicked: remorse.execute("Changing group members is not yet implemented.", function () {})
                }
                MenuItem {
                    // Reused from ProfilePage.qml
                    text: qsTrId("whisperfish-reset-identity-menu")
                    visible: SettingsBridge.debug_mode
                    onClicked: {
                        var sessionMethods = SessionModel;
                        //: Reset identity key remorse message (past tense)
                        //% "Identity key reset"
                        remorse.execute(qsTrId("whisperfish-reset-identity-message"), function () {
                            console.log("Resetting identity key for " + recipient.e164);
                            sessionMethods.removeIdentities(recipient.recipientId);
                        });
                    }
                }
                MenuItem {
                    // Reused from ProfilePage.qml
                    text: qsTrId("whisperfish-reset-session-menu")
                    visible: SettingsBridge.debug_mode
                    onClicked: {
                        var messageMethods = MessageModel;
                        //: Reset secure session remorse message (past tense)
                        //% "Secure session reset"
                        remorse.execute(qsTrId("whisperfish-reset-session-message"), function () {
                            console.log("Resetting secure session with " + recipient.e164);
                            messageMethods.endSession(recipient.recipientId);
                        });
                    }
                }
            }
        }

        ProfilePicture {
            id: avatar
            highlighted: item.down
            labelsHighlighted: highlighted
            imageSource: item.profilePicture
            isGroup: false // groups can't be members of groups
            showInfoMark: false
            anchors {
                verticalCenter: parent.verticalCenter
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
            }
            onPressAndHold: item.openMenu()
            onClicked: item.clicked(null)
        }

        Column {
            anchors {
                verticalCenter: parent.verticalCenter
                left: avatar.right
                right: parent.right
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
            }
            Row {
                spacing: Theme.paddingSmall
                Label {
                    visible: SettingsBridge.debug_mode && !isSelf
                    width: visible ? implicitWidth : 0
                    height: nameLabel.height
                    font.pixelSize: Theme.fontSizeTiny
                    // 0 = Unknown, 1 = Disabled, 2 = Enabled, 3 = Unlimited
                    color: recipient.unidentifiedAccessMode >= 2 ? "green" : "red"
                    text: '🔒'
                }
                Label {
                    id: nameLabel
                    font.pixelSize: Theme.fontSizeMedium
                    text: item.isSelf ? //: Title for the user's entry in a list of group members
                    //% "You"
                    qsTrId("whisperfish-group-member-name-self") : item.isUnknownContact ? // Translated in SessionDelegate.qml
                    qsTrId("whisperfish-recipient-no-name") : name
                }
            }
            Label {
                color: item.down ? Theme.secondaryHighlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                text: recipient.e164 ? recipient.e164 : ''
            }
        }
    }
}
