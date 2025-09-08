import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"

Page {
    id: groupProfile
    objectName: "groupProfilePage"

    // Group wallpapers/background are inherently un-sailfishy. We
    // should show them somewhere, somehow nonetheless - just not as
    // a background image. A group admin should be able to change it, too.

    property QtObject session

    Group {
        id: group
        app: AppState
        groupId: session ? session.groupId : ""
    }

    // For new message notifications
    property int sessionId: !!session ? session.sessionId : -1

    property bool youAreAdmin: groupMembers.youAreAdmin // TODO: This feels like a hack; add to group properties.
    // This variable is needed because MenuItem doesn't see inside SilicaListView.header container
    property int newDuration: -1

    RemorsePopup {
        id: remorse
    }

    SilicaFlickable {
        id: flickable
        anchors.fill: parent
        contentHeight: column.height + groupMembers.height

        VerticalScrollDecorator {
            flickable: flickable
        }

        PullDownMenu {
            MenuItem {
                //: Refresh group menu item
                //% "Refresh group"
                text: qsTrId("whisperfish-group-refresh")
                onClicked: {
                    console.log("Refreshing group for session", sessionId);
                    ClientWorker.refresh_group_v2(sessionId);
                }
            }
            MenuItem {
                //: Leave group menu item
                //% "Leave this group"
                text: qsTrId("whisperfish-group-leave-menu")
                onClicked: {
                    // TODO Leaving a group should *never* delete its messages.
                    //      Two different destructive actions should require two different
                    //      inputs and two confirmations.
                    //      Is it enough to remove the 'remove' line?
                    //: Leave group remorse message (past tense)
                    //% "Left group and deleted all messages"
                    remorse.execute(qsTrId("whisperfish-group-leave-remorse"), function () {
                        console.log("Leaving group");
                        MessageModel.leaveGroup();
                        SessionModel.remove(sessionId);
                        mainWindow.showMainPage();
                    });
                }
            }
            MenuItem {
                // TODO implement in backend
                //: Create invite link menu item
                //% "Create invitation link"
                text: qsTrId("whisperfish-group-invite-link-menu")
                visible: false // TODO
                onClicked: remorse.execute("Changing group members is not yet implemented.", function () {})
            }
            MenuItem {
                // TODO implement in backend
                //: Add group member menu item
                //% "Add Member"
                text: qsTrId("whisperfish-group-add-member-menu")
                visible: false // TODO
                onClicked: remorse.execute("Changing group members is not yet implemented.", function () {})
            }
            MenuItem {
                // Translation in ProfilePage.qml
                text: qsTrId("whisperfish-save-message-expiry")
                visible: youAreAdmin && session != null && groupProfile.newDuration !== session.expiringMessageTimeout
                onClicked: MessageModel.createExpiryUpdate(sessionId, groupProfile.newDuration)
            }
        }

        Column {
            id: column
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }
            spacing: Theme.paddingMedium

            PageHeader {
                title: session.groupName
                description: !session.isGroupV2 ?
                //: Indicator for not yet updated groups
                //% "Not updated to the new group format"
                qsTrId("whisperfish-group-not-updated-to-groupv2") : ""
            }

            ProfilePicture {
                id: groupAvatarItem
                enabled: imageStatus === Image.Ready
                height: 2 * Theme.itemSizeLarge
                width: height
                highlighted: false
                labelsHighlighted: false
                imageSource: !!session.groupId ? SettingsBridge.avatar_dir + "/" + session.groupId : ''
                isGroup: true
                showInfoMark: infoMarkSource !== ''
                infoMarkSource: session.isGroupV2 ? '' : 'image://theme/icon-s-filled-warning'
                infoMarkSize: 0.9 * Theme.iconSizeSmallPlus
                anchors.horizontalCenter: parent.horizontalCenter

                // TODO Implement a new page derived from ViewImagePage for showing
                //      profile pictures. A new action overlay at the bottom can provide
                //      options to change or delete the profile picture.
                //      Note: adding a PullDownMenu would be best but is not possible.
                //      ViewImagePage relies on Flickable and breaks if used with SilicaFlickable,
                //      but PullDownMenu requires a SilicaFlickable as parent.
                onClicked: pageStack.push(Qt.resolvedUrl("ViewImagePage.qml"), {
                    title: session.groupName,
                    path: imageSource
                })
            }

            LinkedEmojiLabel {
                anchors {
                    left: parent.left
                    right: parent.right
                    leftMargin: 2 * Theme.horizontalPageMargin
                    rightMargin: 2 * Theme.horizontalPageMargin
                }
                visible: plainText != ""

                Behavior on height { SmoothedAnimation { duration: 150 } }

                plainText: session.groupDescription ? session.groupDescription : ""
                font.pixelSize: Theme.fontSizeSmall
                emojiSizeMult: 1.0
                horizontalAlignment: Text.AlignHCenter
                linkColor: color
            }

            ExpiringMessagesComboBox {
                enabled: youAreAdmin
                // This height hack is required to prevent the newly-created
                // page from scrolling up a bit when the page is creaged
                // and first getting rendered.
                //
                // Avoid items with dynamic height in headers.
                height: implicitHeight === 0 ? width : implicitHeight
                width: parent.width
                duration: session.expiringMessageTimeout
                onNewDurationChanged: groupProfile.newDuration = newDuration
            }

            ComboBox {
                // XXX Consider separate settings sub-page
                //: Announcements only setting label
                //% "Message sending allowed"
                label: qsTrId("whisperfish-announcements-switch-label")
                value: currentIndex == 1
                    ? qsTrId("whisperfish-announcements-admins-only")
                    : qsTrId("whisperfish-announcements-all-useres")
                property bool announcementsOnly: group.isAnnouncementsOnly
                currentIndex: announcementsOnly ? 1 : 0
                menu: ContextMenu {
                    MenuItem {
                        //: Message sending allowed for all users
                        //% "All users"
                        text: qsTrId("whisperfish-announcements-all-useres")
                    }
                    MenuItem {
                        //: Message sending allowed for admins only
                        //% "Administrators only"
                        text: qsTrId("whisperfish-announcements-admins-only")
                    }
                }
                onCurrentIndexChanged: {
                    if ((currentIndex == 1) != group.isAnnouncementsOnly) {
                        // XXX: sending group updates is not implemented
                        console.log("announcements only mode", currentIndex == 1 ? "enabled" : "disabled")
                    }
                }
                onAnnouncementsOnlyChanged: {
                    if (announcementsOnly) {
                        currentIndex = 1
                    } else {
                        currentIndex = 0
                    }
                }
            }
        }

        GroupMemberListView {
            id: groupMembers

            anchors.top: column.bottom
            width: parent.width
            height: contentHeight

            group: group
        }
    }
}
