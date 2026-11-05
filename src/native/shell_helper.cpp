// Only discover/initialize/invoke. Every visible menu belongs to the GPUI application.
#include "shell_common.hpp"
#include <io.h>
#include <fcntl.h>
#include <unordered_map>
#include <set>
#include <sstream>
struct Node{HMENU parent;UINT position;UINT command;HMENU submenu;};
static std::unordered_map<uint32_t,Node>nodes;static uint32_t nextNode=1;
static void number(std::vector<unsigned char>&b,uint32_t n){for(int i=0;i<4;i++)b.push_back((unsigned char)(n>>(i*8)));}
static void string(std::vector<unsigned char>&b,const std::wstring&s){number(b,(uint32_t)s.size());for(wchar_t c:s){b.push_back((unsigned char)c);b.push_back((unsigned char)(c>>8));}}
static std::wstring verb(IContextMenu*context,UINT command){if(command<1||command>0x7fff)return L"";std::array<wchar_t,512>buffer{};if(FAILED(context->GetCommandString(command-1,GCS_VERBW,nullptr,(LPSTR)buffer.data(),(UINT)buffer.size())))return L"";buffer.back()=0;std::wstring value(buffer.data());for(auto&c:value)c=(wchar_t)towlower(c);return value;}
static bool duplicate(const std::wstring&v){static std::set<std::wstring>values={L"open",L"copy",L"cut",L"paste",L"delete",L"rename",L"properties",L"opennewwindow",L"opennewprocess",L"pintohome",L"unpinfromhome"};return values.count(v)!=0;}
static void publish(HWND owner,IContextMenu*context,HMENU menu,uint32_t parentToken,UINT parentPosition){
 SendMessageW(owner,WM_INITMENUPOPUP,(WPARAM)menu,MAKELPARAM(parentPosition,FALSE));
 int count=GetMenuItemCount(menu);require(count>=0&&count<=512);std::vector<unsigned char>body;number(body,0);uint32_t written=0;
 for(auto i=nodes.begin();i!=nodes.end();){if(i->second.parent==menu)i=nodes.erase(i);else ++i;}
 for(int index=0;index<count;index++){
  MENUITEMINFOW item{};item.cbSize=sizeof(item);item.fMask=MIIM_FTYPE|MIIM_STATE|MIIM_ID|MIIM_SUBMENU|MIIM_STRING;require(GetMenuItemInfoW(menu,index,TRUE,&item));
  bool separator=(item.fType&MFT_SEPARATOR)!=0;std::wstring canonical=separator?L"":verb(context,item.wID);if(!item.hSubMenu&&duplicate(canonical))continue;
  bool unavailable=item.cch>2048;std::wstring label;
  if(!separator&&!unavailable&&item.cch){std::vector<wchar_t>buffer(item.cch+1);item.dwTypeData=buffer.data();item.cch=(UINT)buffer.size();require(GetMenuItemInfoW(menu,index,TRUE,&item));label=buffer.data();}
  if(!separator&&label.empty()){unavailable=true;label=L"此扩展未提供菜单文字";}if(unavailable&&label.empty())label=L"扩展名称过长";
  require(nodes.size()<2048&&nextNode<0xfffffff0);uint32_t id=nextNode++;nodes.emplace(id,Node{menu,(UINT)index,item.wID,item.hSubMenu});
  uint32_t flags=(separator?0:1)|((item.fState&(MFS_DISABLED|MFS_GRAYED))?2:0)|((item.fState&MFS_CHECKED)?4:0)|((item.fType&MFT_RADIOCHECK)?8:0)|(item.hSubMenu?16:0)|(unavailable?32:0);
  number(body,id);number(body,flags);string(body,label);string(body,canonical);written++;require(body.size()<=512*1024);
 }
 memcpy(body.data(),&written,4);std::cout<<"rows "<<parentToken<<" "<<body.size()<<"\n";std::cout.write((char*)body.data(),body.size());std::cout.flush();
}
int wmain(int argc,wchar_t**argv){try{
 require(argc==4);_setmode(_fileno(stdout),_O_BINARY);_setmode(_fileno(stdin),_O_BINARY);SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOGPFAULTERRORBOX);SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);require(SUCCEEDED(CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED)));
 HWND parent=(HWND)(uintptr_t)_wcstoui64(argv[2],nullptr,10);DWORD pid=0;GetWindowThreadProcessId(parent,&pid);require(IsWindow(parent)&&pid==wcstoul(argv[3],nullptr,10));
 HANDLE file=CreateFileW(argv[1],GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);require(file!=INVALID_HANDLE_VALUE);uint32_t count=0;exact(file,&count,4);require(count<=2048);auto folderPath=text(file);std::array<unsigned char,33>folderId;exact(file,folderId.data(),33);require(identity(folderPath)==folderId);
 std::vector<std::wstring>paths;std::vector<std::array<unsigned char,33>>ids;size_t units=folderPath.size();for(uint32_t i=0;i<count;i++){paths.push_back(text(file));units+=paths.back().size();require(units<=256*1024);std::array<unsigned char,33>id;exact(file,id.data(),33);require(identity(paths.back())==id);ids.push_back(id);}CloseHandle(file);
 WNDCLASSW wc{};wc.lpfnWndProc=proc;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"BackleafShellHost";RegisterClassW(&wc);HWND owner=CreateWindowExW(WS_EX_TOOLWINDOW,wc.lpszClassName,L"",WS_POPUP,0,0,1,1,parent,nullptr,wc.hInstance,nullptr);require(owner!=nullptr);
 IShellFolder*desktop=nullptr;IShellFolder*folder=nullptr;require(SUCCEEDED(SHGetDesktopFolder(&desktop)));PIDLIST_ABSOLUTE root=parse(folderPath,folderId);require(SUCCEEDED(desktop->BindToObject(root,nullptr,IID_PPV_ARGS(&folder))));
 IContextMenu*context=nullptr;std::vector<PIDLIST_ABSOLUTE>absolute;std::vector<PCUITEMID_CHILD>children;for(size_t i=0;i<paths.size();i++){auto item=parse(paths[i],ids[i]);require(ILIsParent(root,item,TRUE));absolute.push_back(item);children.push_back(ILFindLastID(item));}
 if(count)require(SUCCEEDED(folder->GetUIObjectOf(owner,count,children.data(),IID_IContextMenu,nullptr,(void**)&context)));else require(SUCCEEDED(folder->CreateViewObject(owner,IID_IContextMenu,(void**)&context)));
 context->QueryInterface(IID_PPV_ARGS(&menu3));if(!menu3)context->QueryInterface(IID_PPV_ARGS(&menu2));HMENU menu=CreatePopupMenu();require(menu!=nullptr);require(SUCCEEDED(context->QueryContextMenu(menu,0,1,0x7fff,CMF_NORMAL|CMF_EXPLORE)));publish(owner,context,menu,0,0);
 HANDLE input=GetStdHandle(STD_INPUT_HANDLE);std::string line;bool closed=false;
 while(!closed){MSG message;while(PeekMessageW(&message,nullptr,0,0,PM_REMOVE)){TranslateMessage(&message);DispatchMessageW(&message);}DWORD available=0;if(!PeekNamedPipe(input,nullptr,0,nullptr,&available,nullptr))break;if(!available){Sleep(5);continue;}char buffer[4096];DWORD read=0;require(ReadFile(input,buffer,available<sizeof(buffer)?available:(DWORD)sizeof(buffer),&read,nullptr));
  for(DWORD i=0;i<read;i++){char c=buffer[i];if(c!='\n'){require(line.size()<4096);line.push_back(c);continue;}if(!line.empty()&&line.back()=='\r')line.pop_back();std::istringstream command(line);std::string action;uint32_t id=0;command>>action>>id;line.clear();
   if(action=="close"){closed=true;break;}auto found=nodes.find(id);require(found!=nodes.end());Node node=found->second;
   if(action=="menu"){require(node.submenu!=nullptr);publish(owner,context,node.submenu,id,node.position);}
   else if(action=="invoke"){
    require(!node.submenu&&IsWindow(parent));GetWindowThreadProcessId(parent,&pid);require(pid==wcstoul(argv[3],nullptr,10));require(identity(folderPath)==folderId);for(size_t n=0;n<paths.size();n++)require(identity(paths[n])==ids[n]);
    MENUITEMINFOW current{};current.cbSize=sizeof(current);current.fMask=MIIM_ID|MIIM_STATE|MIIM_SUBMENU;require(GetMenuItemInfoW(node.parent,node.position,TRUE,&current)&&current.wID==node.command&&!current.hSubMenu&&!(current.fState&(MFS_DISABLED|MFS_GRAYED)));require(node.command>=1&&node.command<=0x7fff);
    SetWindowLongPtrW(owner,GWLP_HWNDPARENT,0);std::cout<<"invoking\n"<<std::flush;SetForegroundWindow(parent);POINT point{};GetCursorPos(&point);CMINVOKECOMMANDINFOEX info{};info.cbSize=sizeof(info);info.fMask=CMIC_MASK_UNICODE|CMIC_MASK_PTINVOKE;info.hwnd=parent;info.lpVerb=MAKEINTRESOURCEA(node.command-1);info.lpVerbW=MAKEINTRESOURCEW(node.command-1);info.nShow=SW_SHOWNORMAL;info.ptInvoke=point;require(SUCCEEDED(context->InvokeCommand((LPCMINVOKECOMMANDINFO)&info)));closed=true;break;
   }else require(false);
  }
 }
 DestroyMenu(menu);if(menu3)menu3->Release();if(menu2)menu2->Release();context->Release();folder->Release();desktop->Release();for(auto p:absolute)CoTaskMemFree(p);CoTaskMemFree(root);DestroyWindow(owner);CoUninitialize();std::cout<<"closed\n"<<std::flush;return 0;
 }catch(...){std::cout<<"error\n"<<std::flush;return 1;}}
