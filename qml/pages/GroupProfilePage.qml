import QtQuick 2.2
import Sailfish.Silica 1.0
import Sailfish.TextLinking 1.0
import "../components"

Page {
    id: root

    // Group wallpapers/background are inherently un-sailfishy. We
    // should show them somewhere, somehow nonetheless - just not as
    // a background image. A group admin should be able to change it, too.
    /* property string groupWallpaper: '' */

    property string groupName: MessageModel.peerName
    property string groupDescription: '' // TODO implement in backend
    property string groupAvatar: '' // TODO implement in backend

    readonly property string groupMembers: MessageModel.groupMembers
    onGroupMembersChanged: contactListModel.refresh()
    Component.onCompleted: contactListModel.refresh()

    ListModel {
        id: contactListModel
        function refresh() {
            clear()
            var lst = groupMembers.split(",")
            for (var i = 0; i < lst.length; i++) {
                var member = resolvePeopleModel.personByPhoneNumber(lst[i], true)
                var name = member ? member.displayLabel : lst[i]
                var isUnknown = false // checked below
                var isVerified = false // TODO implement in backend

                if (!lst[i]) continue // skip empty/invalid values

                // TODO localId is available but not used by the backend, i.e. always empty
                //      Related to #138. We need a way to check our own id.
                // TODO 'self' should always be the first entry in the list because the entry
                //      will not be clickable and act as a header.
                var isSelf = (lst[i] === SetupWorker.localId) // currently always false

                if (name === lst[i]) {
                    // TODO Use nickname defined in the profile (#192)
                    // Unknown contact
                    //: Unknown contact in group member list
                    //% "Unknown"
                    name = qsTrId("whisperfish-unknown-contact")
                    isUnknown = true
                }

                append({"contactId": lst[i],
                           "name": name,
                           "isUnknown": isUnknown,
                           "isVerified": isVerified,
                           "isSelf": isSelf,
                       })
            }
        }
    }

    RemorsePopup { id: remorse }

    SilicaListView {
        id: flick
        anchors.fill: parent
        model: contactListModel
        header: Column {
            width: parent.width

            PageHeader {
                title: groupName
                description: {
                    // TODO Consider removing the description label for updated groups
                    //      because this should become the standard. Only show a hint
                    //      for non-updated groups.
                    // This can be expanded once there are more group versions.
                    //: Indicator for updated groups
                    //% "Updated to the new group format"
                    if (MessageModel.groupV2) qsTrId("whisperfish-group-updated-to-groupv2")
                    //: Indicator for not yet updated groups
                    //% "Not updated to the new group format"
                    else if (MessageModel.groupV1) qsTrId("whisperfish-group-not-updated-to-groupv2")
                    else "" // we could shown an error here, or we don't
                }
            }

            ProfilePicture {
                id: groupAvatarItem
                height: 2*Theme.itemSizeLarge
                width: height
                highlighted: false
                labelsHighlighted: false
                imageSource: groupAvatar
                isGroup: true
                showInfoMark: true
                infoMark.source: 'image://theme/icon-s-group-chat' // edit
                infoMarkSize: 0.9*Theme.iconSizeSmallPlus
                anchors.horizontalCenter: parent.horizontalCenter
                onClicked: {
                    // TODO Implement a new page derived from ViewImagePage for showing
                    //      profile pictures. A new action overlay at the bottom can provide
                    //      options to change or delete the profile picture.
                    //      Note: adding a PullDownMenu would be best but is not possible.
                    //      ViewImagePage relies on Flickable and breaks if used with SilicaFlickable,
                    //      but PullDownMenu requires a SilicaFlickable as parent.
                    if (groupAvatar === '') {
                        remorse.execute("Changing the avatar is not yet implemented.", function() {})
                        return
                    }
                    pageStack.push(Qt.resolvedUrl("ViewImagePage.qml"), {
                                   'title': groupName, 'source': groupAvatar })
                }
            }

            Item {
                width: parent.width
                height: Theme.paddingLarge
            }

            Item {
                width: parent.width
                height: descriptionLabel.height
                Behavior on height { SmoothedAnimation { duration: 150 } }
                clip: true

                LinkedEmojiLabel {
                    id: descriptionLabel
                    property bool expanded: false

                    // TODO: the description should be editable if the user has the
                    //       appropriate permission (either admin, or all are allowed to edit)

                    x: Theme.horizontalPageMargin
                    width: parent.width-2*Theme.horizontalPageMargin
                    plainText: groupDescription
                    font.pixelSize: Theme.fontSizeSmall
                    // enableElide: Text.ElideRight -- no elide to enable dynamic height
                    // height: maximumLineCount*font.pixelSize
                    maximumLineCount: expanded ? 100000 : 5
                    emojiSizeMult: 1.0
                    horizontalAlignment: Text.AlignLeft
                    color: expandDescriptionArea.pressed ?
                               Theme.secondaryHighlightColor :
                               Theme.secondaryColor
                    linkColor: color

                    MouseArea {
                        // no BackgroundItem to simplify placement, and we don't need the background
                        id: expandDescriptionArea
                        anchors.fill: parent
                        enabled: parent.truncated || parent.expanded
                        onClicked: parent.expanded = !parent.expanded
                    }
                }

                Label {
                    anchors {
                        bottom: descriptionLabel.bottom
                        right: descriptionLabel.right
                    }
                    font.pixelSize: Theme.fontSizeExtraSmall
                    text: "\u2022 \u2022 \u2022" // three dots
                    visible: descriptionLabel.truncated || descriptionLabel.expanded
                    color: expandDescriptionArea.pressed ?
                               Theme.highlightColor :
                               Theme.primaryColor
                }

                OpacityRampEffect {
                    direction: OpacityRamp.TopToBottom
                    offset: 0.5
                    slope: 2
                    sourceItem: descriptionLabel
                    enabled: descriptionLabel.truncated &&
                             !descriptionLabel.expanded
                }
            }

            Item { width: parent.width; height: Theme.paddingLarge }
        }

        VerticalScrollDecorator { flickable: flick }

        PullDownMenu {
            MenuItem {
                //: Leave group menu item
                //% "Leave this group"
                text: qsTrId("whisperfish-group-leave-menu")
                onClicked: {
                    // TODO Leaving a group should *never* delete its messages.
                    //      Two different destructive actions should require two different
                    //      inputs and two confirmations.
                    //      Is it enough to remove the 'removeById' line?
                    //: Leave group remorse message
                    //% "Leaving group and removing ALL messages!"
                    remorse.execute(qsTrId("whisperfish-group-leave-remorse"),
                                    function() {
                                        console.log("Leaving group")
                                        MessageModel.leaveGroup()
                                        SessionModel.removeById(MessageModel.sessionId)
                                        mainWindow.showMainPage()
                                    })
                }
            }
            MenuItem {
                // TODO implement in backend
                //: Create invite link menu item
                //% "Create invitation link"
                text: qsTrId("whisperfish-group-invite-link-menu")
                onClicked: remorse.execute("Changing group members is not yet implemented.", function() {})
            }
            MenuItem {
                // TODO implement in backend
                //: Add group member menu item
                //% "Add Member"
                text: qsTrId("whisperfish-group-add-member-menu")
                onClicked: remorse.execute("Changing group members is not yet implemented.", function() {})
            }
        }

        delegate: ListItem {
            id: item
            contentHeight: Theme.itemSizeMedium
            enabled: !isSelf

            property bool selfIsAdmin: false // TODO implement in backend
            property bool isUnknownContact: model.isUnknown
            property bool isVerified: model.isVerified
            property bool isSelf: model.isSelf

            // TODO This is an ugly hack that relies on contactId being a phone number.
            //      - Remove if/when contacts move to UUIDs
            //      - Implement custom contact page for Whisperfish contacts
            onClicked: phonenumberLink.linkActivated('tel:'+contactId)

            menu: Component {
                ContextMenu {
                    MenuItem {
                        // TODO Implement a way to open a new chat with someone, or open
                        //      an existing chat. This requires better handling of sessions (#105, #183)
                        //: Menu item to start a private chat with a group member
                        //% "Message to %1"
                        text: qsTrId("whisperfish-group-member-menu-direct-message").arg(
                                  isUnknownContact ? contactId : name)
                        // TODO Remove the conditional once contact ids are no longer phone numbers,
                        //      and once profiles (nicknames) are implemented.
                        onClicked: remorse.execute("Directly opening a chat is not yet implemented.", function() {})
                    }
                    MenuItem {
                        //: Menu item to save a group member to the local address book
                        //% "Add to contacts"
                        text: qsTrId("whisperfish-group-member-menu-save-contact")
                        visible: isUnknownContact
                        onClicked: item.clicked(null) // show contact page
                    }
                    MenuItem {
                        //: Menu item to verify safety numbers with a group member
                        //% "Verify safety number"
                        text: qsTrId("whisperfish-group-member-menu-verify-fingerprint")
                        visible: !isVerified
                        onClicked: remorse.execute("Directly verifying the safety number is not yet implemented.", function() {})
                        // TODO We cannot open the verification page because we would have to
                        //      reload the message model, which currently holds this group's messages.
                        //      This is blocked by #105 and maybe #183.
                        //
                        // Not possible:
                        //      MessageModel.load(contactId, ContactModel.name(contactId))
                        //      pageStack.push(Qt.resolvedUrl("../pages/VerifyIdentity.qml"))
                    }
                    MenuItem {
                        //: Menu item to remove a member from a group (requires admin privileges)
                        //% "Remove from this group"
                        text: qsTrId("whisperfish-group-member-menu-remove-from-group")
                        visible: selfIsAdmin
                        onClicked: remorse.execute("Changing group members is not yet implemented.", function() {})
                    }
                }
            }

            Row {
                width: parent.width - 2*Theme.horizontalPageMargin
                height: parent.height
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingLarge

                ProfilePicture {
                    highlighted: item.down
                    labelsHighlighted: highlighted
                    imageSource: '' // TODO implement somewhere
                    isGroup: false // groups can't be members of groups
                    showInfoMark: false
                    anchors.verticalCenter: parent.verticalCenter
                    onPressAndHold: item.openMenu()
                    onClicked: item.clicked(null)
                }

                Column {
                    anchors {
                        verticalCenter: parent.verticalCenter
                        // where does the extra top padding come from?
                        verticalCenterOffset: -Theme.paddingSmall
                    }
                    Label {
                        font.pixelSize: Theme.fontSizeMedium
                        text: item.isSelf ?
                                  //: Title for the user's entry in a list of group members
                                  //% "You"
                                  qsTrId("whisperfish-group-member-name-self") :
                                  name
                    }
                    LinkedText {
                        id: phonenumberLink
                        linkColor: color
                        color: item.down ? Theme.secondaryHighlightColor :
                                           Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        plainText: contactId
                    }
                }
            }
        }
    }
}
